use polars::chunked_array::builder::get_list_builder;
use polars::prelude::*;
use serde::{Deserialize, Serialize};

use crate::record::{field_in, Value};
use crate::sexp::Sexp;
use crate::temporal::{self, Unit};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum DtypeSpec {
    String,
    Boolean,
    Int8,
    Int16,
    Int32,
    Int64,
    Int128,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    UInt128,
    Float32,
    Float64,
    List {
        inner: Box<DtypeSpec>,
    },
    Array {
        inner: Box<DtypeSpec>,
        width: usize,
    },
    Struct {
        fields: Vec<FieldSpec>,
    },
    Tagged {
        variants: Vec<FieldSpec>,
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
    Time {
        format: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    pub name: String,
    pub dtype: DtypeSpec,
}

const TAG_FIELD: &str = "tag";

fn struct_dtype(fields: &[FieldSpec]) -> PolarsResult<DataType> {
    Ok(DataType::Struct(
        fields
            .iter()
            .map(|f| Ok(Field::new(f.name.as_str().into(), f.dtype.dtype()?)))
            .collect::<PolarsResult<_>>()?,
    ))
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
            DtypeSpec::Int128 => DataType::Int128,
            DtypeSpec::UInt8 => DataType::UInt8,
            DtypeSpec::UInt16 => DataType::UInt16,
            DtypeSpec::UInt32 => DataType::UInt32,
            DtypeSpec::UInt64 => DataType::UInt64,
            DtypeSpec::UInt128 => DataType::UInt128,
            DtypeSpec::Float32 => DataType::Float32,
            DtypeSpec::Float64 => DataType::Float64,
            DtypeSpec::List { inner } => DataType::List(Box::new(inner.dtype()?)),
            DtypeSpec::Array { inner, width } => DataType::Array(Box::new(inner.dtype()?), *width),
            DtypeSpec::Struct { fields } => struct_dtype(fields)?,
            DtypeSpec::Tagged { variants } => {
                let mut fields = vec![Field::new(TAG_FIELD.into(), DataType::String)];
                for v in variants {
                    fields.push(Field::new(v.name.as_str().into(), v.dtype.dtype()?));
                }
                DataType::Struct(fields)
            }
            DtypeSpec::Date { .. } => DataType::Date,
            DtypeSpec::Datetime {
                unit, time_zone, ..
            } => DataType::Datetime(unit.polars(), TimeZone::opt_try_new(time_zone.clone())?),
            DtypeSpec::Duration { unit } => DataType::Duration(unit.polars()),
            DtypeSpec::Time { .. } => DataType::Time,
        })
    }
}

pub struct Decoder {
    pub strict: bool,
}

fn mismatch(value: Value, dtype: &DataType) -> PolarsError {
    polars_err!(ComputeError: "cannot decode `{}` as {}", value.to_text_with_bare_atom(), dtype)
}

fn assemble_struct<T>(
    name: &PlSmallStr,
    fields: &[Series],
    rows: &[Option<T>],
) -> PolarsResult<Series> {
    let is_present: BooleanChunked = rows.iter().map(|r| Some(r.is_some())).collect();
    let validity = is_present.downcast_as_array().values().clone();
    Ok(
        StructChunked::from_series(name.clone(), rows.len(), fields.iter())?
            .with_outer_validity(Some(validity))
            .into_series(),
    )
}

fn is_keyed_payload(payload: &[Sexp], fields: &[FieldSpec]) -> bool {
    !payload.is_empty()
        && payload.iter().all(|item| match item {
            Sexp::List(entry) => match entry.first() {
                Some(Sexp::Atom(key)) => fields.iter().any(|f| &f.name == key),
                _ => false,
            },
            Sexp::Atom(_) => false,
        })
}

impl Decoder {
    fn present_or_mismatch<'a, T>(
        &self,
        vals: &[Option<Value<'a>>],
        dtype: &DataType,
        convert: impl Fn(Value<'a>) -> Option<T>,
    ) -> PolarsResult<Vec<Option<T>>> {
        vals.iter()
            .map(|v| match v {
                None => Ok(None),
                Some(v) => match convert(*v) {
                    Some(t) => Ok(Some(t)),
                    None if self.strict && !v.is_nil() => Err(mismatch(*v, dtype)),
                    None => Ok(None),
                },
            })
            .collect()
    }

    fn atoms<'a, T>(
        &self,
        vals: &[Option<Value<'a>>],
        dtype: &DataType,
        parse: impl Fn(&str) -> Option<T>,
    ) -> PolarsResult<Vec<Option<T>>> {
        self.present_or_mismatch(vals, dtype, |v| parse(v.as_atom()?))
    }

    fn cast(&self, physical: Series, dtype: &DataType) -> PolarsResult<Series> {
        if physical.dtype() == dtype {
            Ok(physical)
        } else if self.strict {
            physical.strict_cast(dtype)
        } else {
            physical.cast(dtype)
        }
    }

    pub fn decode(
        &self,
        name: PlSmallStr,
        vals: &[Option<Value>],
        spec: &DtypeSpec,
    ) -> PolarsResult<Series> {
        let dtype = spec.dtype()?;
        let physical = match spec {
            DtypeSpec::String => vals
                .iter()
                .map(|v| {
                    v.filter(|v| !v.is_nil())
                        .map(|v| v.to_text_with_bare_atom())
                })
                .collect::<StringChunked>()
                .into_series(),
            DtypeSpec::Boolean => self
                .atoms(vals, &dtype, |atom| match atom {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => None,
                })?
                .into_iter()
                .collect::<BooleanChunked>()
                .into_series(),
            DtypeSpec::Int8 | DtypeSpec::Int16 | DtypeSpec::Int32 | DtypeSpec::Int64 => {
                Int64Chunked::from_iter(self.atoms(vals, &dtype, |a| a.parse::<i64>().ok())?)
                    .into_series()
            }
            DtypeSpec::Int128 => {
                Int128Chunked::from_iter(self.atoms(vals, &dtype, |a| a.parse::<i128>().ok())?)
                    .into_series()
            }
            DtypeSpec::UInt8 | DtypeSpec::UInt16 | DtypeSpec::UInt32 | DtypeSpec::UInt64 => {
                UInt64Chunked::from_iter(self.atoms(vals, &dtype, |a| a.parse::<u64>().ok())?)
                    .into_series()
            }
            DtypeSpec::UInt128 => {
                UInt128Chunked::from_iter(self.atoms(vals, &dtype, |a| a.parse::<u128>().ok())?)
                    .into_series()
            }
            DtypeSpec::Float32 | DtypeSpec::Float64 => {
                Float64Chunked::from_iter(self.atoms(vals, &dtype, |a| a.parse::<f64>().ok())?)
                    .into_series()
            }
            DtypeSpec::List { inner } => self.decode_list(&name, vals, &dtype, inner, None)?,
            DtypeSpec::Array { inner, width } => {
                self.decode_list(&name, vals, &dtype, inner, Some(*width))?
            }
            DtypeSpec::Struct { fields } => self.decode_struct(&name, vals, &dtype, fields)?,
            DtypeSpec::Tagged { variants } => self.decode_tagged(&name, vals, &dtype, variants)?,
            DtypeSpec::Date { format } => {
                Int32Chunked::from_iter(self.atoms(vals, &dtype, |a| {
                    temporal::parse_days_since_epoch(a, format.as_deref())
                })?)
                .into_series()
            }
            DtypeSpec::Datetime { unit, format, .. } => {
                let wall_clock_zone = match &dtype {
                    DataType::Datetime(_, Some(tz)) => Some(tz.to_chrono()?),
                    _ => None,
                };
                Int64Chunked::from_iter(self.atoms(vals, &dtype, |a| {
                    temporal::parse_timestamp(a, format.as_deref(), *unit, wall_clock_zone.as_ref())
                })?)
                .into_series()
            }
            DtypeSpec::Duration { unit } => {
                Int64Chunked::from_iter(
                    self.atoms(vals, &dtype, |a| temporal::parse_duration(a, *unit))?,
                )
                .into_series()
            }
            DtypeSpec::Time { format } => {
                Int64Chunked::from_iter(self.atoms(vals, &dtype, |a| {
                    temporal::parse_nanos_since_midnight(a, format.as_deref())
                })?)
                .into_series()
            }
        };
        Ok(self.cast(physical, &dtype)?.with_name(name))
    }

    fn decode_list<'a>(
        &self,
        name: &PlSmallStr,
        vals: &[Option<Value<'a>>],
        dtype: &DataType,
        inner: &DtypeSpec,
        required_len: Option<usize>,
    ) -> PolarsResult<Series> {
        let rows = self.present_or_mismatch(vals, dtype, |v| {
            v.as_list_elements()
                .filter(|items| required_len.is_none_or(|len| items.len() == len))
        })?;
        let children: Vec<Option<Value>> = rows
            .iter()
            .flatten()
            .flat_map(|items| items.iter().map(|c| Some(Value::Node(c))))
            .collect();
        let decoded_children = self.decode(PlSmallStr::EMPTY, &children, inner)?;
        let mut builder =
            get_list_builder(&inner.dtype()?, children.len(), rows.len(), name.clone());
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

    fn decode_struct<'a>(
        &self,
        name: &PlSmallStr,
        vals: &[Option<Value<'a>>],
        dtype: &DataType,
        fields: &[FieldSpec],
    ) -> PolarsResult<Series> {
        let records = self.present_or_mismatch(vals, dtype, |v| {
            v.as_items().filter(|items| !items.is_empty()).map(|_| v)
        })?;
        let field_series = fields
            .iter()
            .map(|f| {
                let field_vals: Vec<Option<Value>> = records
                    .iter()
                    .map(|r| r.and_then(|r| r.record_field(&f.name)))
                    .collect();
                self.decode(f.name.as_str().into(), &field_vals, &f.dtype)
            })
            .collect::<PolarsResult<Vec<_>>>()?;
        assemble_struct(name, &field_series, &records)
    }

    fn decode_tagged<'a>(
        &self,
        name: &PlSmallStr,
        vals: &[Option<Value<'a>>],
        dtype: &DataType,
        variants: &[FieldSpec],
    ) -> PolarsResult<Series> {
        let tagged =
            self.present_or_mismatch(vals, dtype, |v| match v.as_items()?.split_first()? {
                (Sexp::Atom(tag), payload) => {
                    Some((variants.iter().position(|f| &f.name == tag)?, payload))
                }
                (Sexp::List(_), _) => None,
            })?;
        let tags: StringChunked = tagged
            .iter()
            .map(|t| t.map(|(i, _)| variants[i].name.as_str()))
            .collect();
        let mut fields = vec![tags.with_name(TAG_FIELD.into()).into_series()];
        for (i, variant) in variants.iter().enumerate() {
            let payloads: Vec<Option<&[Sexp]>> = tagged
                .iter()
                .map(|t| t.filter(|(j, _)| *j == i).map(|(_, payload)| payload))
                .collect();
            fields.push(self.decode_payload(&variant.name, &payloads, &variant.dtype)?);
        }
        assemble_struct(name, &fields, &tagged)
    }

    fn decode_payload(
        &self,
        name: &str,
        payloads: &[Option<&[Sexp]>],
        spec: &DtypeSpec,
    ) -> PolarsResult<Series> {
        let DtypeSpec::Struct { fields } = spec else {
            let vals: Vec<Option<Value>> =
                payloads.iter().map(|p| p.map(Value::FieldValues)).collect();
            return self.decode(name.into(), &vals, spec);
        };
        let dtype = spec.dtype()?;
        let payloads = self.present_or_mismatch(
            &payloads
                .iter()
                .map(|p| p.map(Value::FieldValues))
                .collect::<Vec<_>>(),
            &dtype,
            |v| {
                let payload = v.as_field_values()?;
                let positional_fits = payload.len() <= fields.len();
                (is_keyed_payload(payload, fields) || positional_fits).then_some(payload)
            },
        )?;
        let field_series = fields
            .iter()
            .enumerate()
            .map(|(position, f)| {
                let field_vals: Vec<Option<Value>> = payloads
                    .iter()
                    .map(|p| {
                        let payload = (*p)?;
                        if is_keyed_payload(payload, fields) {
                            field_in(payload, &f.name)
                        } else {
                            payload.get(position).map(Value::Node)
                        }
                    })
                    .collect();
                self.decode(f.name.as_str().into(), &field_vals, &f.dtype)
            })
            .collect::<PolarsResult<Vec<_>>>()?;
        assemble_struct(&name.into(), &field_series, &payloads)
    }
}
