use polars::chunked_array::builder::get_list_builder;
use polars::prelude::*;
use serde::Deserialize;

use crate::record::Value;
use crate::temporal::{self, Unit};

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum DtypeSpec {
    String,
    Boolean,
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    List {
        inner: Box<DtypeSpec>,
    },
    Struct {
        fields: Vec<FieldSpec>,
    },
    Date {
        format: Option<String>,
    },
    Datetime {
        unit: Unit,
        time_zone: Option<String>,
        format: Option<String>,
    },
    Duration {
        unit: Unit,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldSpec {
    pub name: String,
    pub dtype: DtypeSpec,
}

impl DtypeSpec {
    pub fn dtype(&self) -> PolarsResult<DataType> {
        Ok(match self {
            DtypeSpec::String => DataType::String,
            DtypeSpec::Boolean => DataType::Boolean,
            DtypeSpec::Int8 => DataType::Int8,
            DtypeSpec::Int16 => DataType::Int16,
            DtypeSpec::Int32 => DataType::Int32,
            DtypeSpec::Int64 => DataType::Int64,
            DtypeSpec::UInt8 => DataType::UInt8,
            DtypeSpec::UInt16 => DataType::UInt16,
            DtypeSpec::UInt32 => DataType::UInt32,
            DtypeSpec::UInt64 => DataType::UInt64,
            DtypeSpec::Float32 => DataType::Float32,
            DtypeSpec::Float64 => DataType::Float64,
            DtypeSpec::List { inner } => DataType::List(Box::new(inner.dtype()?)),
            DtypeSpec::Struct { fields } => DataType::Struct(
                fields
                    .iter()
                    .map(|f| Ok(Field::new(f.name.as_str().into(), f.dtype.dtype()?)))
                    .collect::<PolarsResult<_>>()?,
            ),
            DtypeSpec::Date { .. } => DataType::Date,
            DtypeSpec::Datetime {
                unit, time_zone, ..
            } => DataType::Datetime(unit.polars(), TimeZone::opt_try_new(time_zone.clone())?),
            DtypeSpec::Duration { unit } => DataType::Duration(unit.polars()),
        })
    }
}

fn atoms<'a>(vals: &'a [Option<Value<'a>>]) -> impl Iterator<Item = Option<&'a str>> + 'a {
    vals.iter().map(|v| v.and_then(|v| v.as_atom()))
}

pub fn decode(name: PlSmallStr, vals: &[Option<Value>], spec: &DtypeSpec) -> PolarsResult<Series> {
    let dtype = spec.dtype()?;
    let physical = match spec {
        DtypeSpec::String => vals
            .iter()
            .map(|v| v.map(|v| v.to_text_with_bare_atom()))
            .collect::<StringChunked>()
            .into_series(),
        DtypeSpec::Boolean => atoms(vals)
            .map(|atom| match atom {
                Some("true") => Some(true),
                Some("false") => Some(false),
                _ => None,
            })
            .collect::<BooleanChunked>()
            .into_series(),
        DtypeSpec::Int8 | DtypeSpec::Int16 | DtypeSpec::Int32 | DtypeSpec::Int64 => atoms(vals)
            .map(|atom| atom?.parse::<i64>().ok())
            .collect::<Int64Chunked>()
            .into_series(),
        DtypeSpec::UInt8 | DtypeSpec::UInt16 | DtypeSpec::UInt32 | DtypeSpec::UInt64 => atoms(vals)
            .map(|atom| atom?.parse::<u64>().ok())
            .collect::<UInt64Chunked>()
            .into_series(),
        DtypeSpec::Float32 | DtypeSpec::Float64 => atoms(vals)
            .map(|atom| atom?.parse::<f64>().ok())
            .collect::<Float64Chunked>()
            .into_series(),
        DtypeSpec::List { inner } => decode_list(&name, vals, inner)?,
        DtypeSpec::Struct { fields } => decode_struct(&name, vals, fields)?,
        DtypeSpec::Date { format } => atoms(vals)
            .map(|atom| temporal::parse_days_since_epoch(atom?, format.as_deref()))
            .collect::<Int32Chunked>()
            .into_series(),
        DtypeSpec::Datetime { unit, format, .. } => {
            let wall_clock_zone = match &dtype {
                DataType::Datetime(_, Some(tz)) => Some(tz.to_chrono()?),
                _ => None,
            };
            atoms(vals)
                .map(|atom| {
                    temporal::parse_timestamp(
                        atom?,
                        format.as_deref(),
                        *unit,
                        wall_clock_zone.as_ref(),
                    )
                })
                .collect::<Int64Chunked>()
                .into_series()
        }
        DtypeSpec::Duration { unit } => atoms(vals)
            .map(|atom| temporal::parse_duration(atom?, *unit))
            .collect::<Int64Chunked>()
            .into_series(),
    };
    Ok(cast_to_target_dtype(physical, &dtype)?.with_name(name))
}

fn cast_to_target_dtype(physical: Series, dtype: &DataType) -> PolarsResult<Series> {
    if physical.dtype() == dtype {
        Ok(physical)
    } else {
        physical.cast(dtype)
    }
}

fn decode_list(
    name: &PlSmallStr,
    vals: &[Option<Value>],
    inner: &DtypeSpec,
) -> PolarsResult<Series> {
    let rows: Vec<Option<&[_]>> = vals
        .iter()
        .map(|v| v.and_then(|v| v.as_list_elements()))
        .collect();
    let children: Vec<Option<Value>> = rows
        .iter()
        .flatten()
        .flat_map(|items| items.iter().map(|c| Some(Value::Node(c))))
        .collect();
    let decoded_children = decode(PlSmallStr::EMPTY, &children, inner)?;
    let mut builder = get_list_builder(&inner.dtype()?, children.len(), rows.len(), name.clone());
    let mut offset = 0i64;
    for row in &rows {
        match row {
            Some(items) => {
                builder.append_series(&decoded_children.slice(offset, items.len()))?;
                offset += items.len() as i64;
            }
            None => builder.append_null(),
        }
    }
    Ok(builder.finish().into_series())
}

fn decode_struct(
    name: &PlSmallStr,
    vals: &[Option<Value>],
    fields: &[FieldSpec],
) -> PolarsResult<Series> {
    let is_record: BooleanChunked = vals
        .iter()
        .map(|v| Some(v.is_some_and(|v| v.as_items().is_some())))
        .collect();
    let field_series = fields
        .iter()
        .map(|f| {
            let field_vals: Vec<Option<Value>> = vals
                .iter()
                .map(|v| v.and_then(|v| v.record_field(&f.name)))
                .collect();
            decode(f.name.as_str().into(), &field_vals, &f.dtype)
        })
        .collect::<PolarsResult<Vec<_>>>()?;
    let validity = is_record.downcast_as_array().values().clone();
    Ok(
        StructChunked::from_series(name.clone(), vals.len(), field_series.iter())?
            .with_outer_validity(Some(validity))
            .into_series(),
    )
}
