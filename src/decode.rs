//! Decode parsed S-expressions straight into a target Polars dtype.

use polars::chunked_array::builder::get_list_builder;
use polars::prelude::*;
use serde::Deserialize;

use crate::sexp::Value;

/// The target dtype, as sent from Python (see `_dtype_spec` in `__init__.py`).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Spec {
    String,
    Boolean,
    Int { bits: u8, signed: bool },
    Float { bits: u8 },
    List { inner: Box<Spec> },
    Struct { fields: Vec<FieldSpec> },
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldSpec {
    pub name: String,
    pub dtype: Spec,
}

impl Spec {
    pub fn dtype(&self) -> PolarsResult<DataType> {
        Ok(match self {
            Spec::String => DataType::String,
            Spec::Boolean => DataType::Boolean,
            Spec::Int { bits, signed } => match (bits, signed) {
                (8, true) => DataType::Int8,
                (16, true) => DataType::Int16,
                (32, true) => DataType::Int32,
                (64, true) => DataType::Int64,
                (8, false) => DataType::UInt8,
                (16, false) => DataType::UInt16,
                (32, false) => DataType::UInt32,
                (64, false) => DataType::UInt64,
                _ => polars_bail!(InvalidOperation: "unsupported integer width {bits}"),
            },
            Spec::Float { bits: 32 } => DataType::Float32,
            Spec::Float { bits: 64 } => DataType::Float64,
            Spec::Float { bits } => {
                polars_bail!(InvalidOperation: "unsupported float width {bits}")
            }
            Spec::List { inner } => DataType::List(Box::new(inner.dtype()?)),
            Spec::Struct { fields } => DataType::Struct(
                fields
                    .iter()
                    .map(|f| Ok(Field::new(f.name.as_str().into(), f.dtype.dtype()?)))
                    .collect::<PolarsResult<_>>()?,
            ),
        })
    }
}

/// Decode one value per row. Rows that are missing or don't fit the spec are null.
///
/// - String: atoms as-is, lists as canonical S-expression text.
/// - Boolean: the atoms `true` / `false`.
/// - Int / Float: atoms parsed as numbers (out-of-range values become null).
/// - List: the elements of a list.
/// - Struct: a record `((field value) ...)`; a field with several values,
///   like `(tags a b)`, is treated as the list of those values.
pub fn decode(name: PlSmallStr, vals: &[Option<Value>], spec: &Spec) -> PolarsResult<Series> {
    let dtype = spec.dtype()?;
    let out = match spec {
        Spec::String => vals
            .iter()
            .map(|v| v.map(|v| v.to_text()))
            .collect::<StringChunked>()
            .into_series(),
        Spec::Boolean => vals
            .iter()
            .map(|v| match v.and_then(|v| v.as_atom()) {
                Some("true") => Some(true),
                Some("false") => Some(false),
                _ => None,
            })
            .collect::<BooleanChunked>()
            .into_series(),
        Spec::Int { signed: true, .. } => vals
            .iter()
            .map(|v| v.and_then(|v| v.as_atom()?.parse::<i64>().ok()))
            .collect::<Int64Chunked>()
            .into_series(),
        Spec::Int { signed: false, .. } => vals
            .iter()
            .map(|v| v.and_then(|v| v.as_atom()?.parse::<u64>().ok()))
            .collect::<UInt64Chunked>()
            .into_series(),
        Spec::Float { .. } => vals
            .iter()
            .map(|v| v.and_then(|v| v.as_atom()?.parse::<f64>().ok()))
            .collect::<Float64Chunked>()
            .into_series(),
        Spec::List { inner } => {
            let rows: Vec<Option<&[_]>> =
                vals.iter().map(|v| v.and_then(|v| v.as_items())).collect();
            let children: Vec<Option<Value>> = rows
                .iter()
                .flatten()
                .flat_map(|items| items.iter().map(|c| Some(Value::One(c))))
                .collect();
            let inner_series = decode(PlSmallStr::EMPTY, &children, inner)?;
            let mut builder =
                get_list_builder(&inner.dtype()?, children.len(), rows.len(), name.clone());
            let mut offset = 0i64;
            for row in &rows {
                match row {
                    Some(items) => {
                        builder.append_series(&inner_series.slice(offset, items.len()))?;
                        offset += items.len() as i64;
                    }
                    None => builder.append_null(),
                }
            }
            builder.finish().into_series()
        }
        Spec::Struct { fields } => {
            let present: BooleanChunked = vals
                .iter()
                .map(|v| Some(v.is_some_and(|v| v.as_items().is_some())))
                .collect();
            let field_series = fields
                .iter()
                .map(|f| {
                    let field_vals: Vec<Option<Value>> = vals
                        .iter()
                        .map(|v| v.and_then(|v| v.field(&f.name)))
                        .collect();
                    decode(f.name.as_str().into(), &field_vals, &f.dtype)
                })
                .collect::<PolarsResult<Vec<_>>>()?;
            let validity = present.downcast_as_array().values().clone();
            StructChunked::from_series(name.clone(), vals.len(), field_series.iter())?
                .with_outer_validity(Some(validity))
                .into_series()
        }
    };
    // Narrow numeric types (e.g. Int64 -> Int8); out-of-range values become null.
    let out = if out.dtype() == &dtype {
        out
    } else {
        out.cast(&dtype)?
    };
    Ok(out.with_name(name))
}
