use polars::prelude::*;
use pyo3_polars::export::polars_arrow::array::Array;

use crate::sexp::Sexp;
use crate::temporal::{self, Unit};

fn atoms(texts: impl Iterator<Item = Option<String>>) -> Vec<Option<Sexp>> {
    texts.map(|t| t.map(Sexp::Atom)).collect()
}

fn nil() -> Sexp {
    Sexp::List(Vec::new())
}

fn entry(key: &str, value: Sexp) -> Sexp {
    Sexp::List(vec![Sexp::Atom(key.to_string()), value])
}

pub fn encode(s: &Series) -> PolarsResult<Vec<Option<Sexp>>> {
    Ok(match s.dtype() {
        DataType::Null => vec![None; s.len()],
        DataType::String => atoms(s.str()?.iter().map(|v| v.map(str::to_string))),
        DataType::Date => atoms(
            s.date()?
                .physical()
                .iter()
                .map(|d| temporal::format_date(d?)),
        ),
        DataType::Datetime(unit, tz) => {
            let zone = tz.as_ref().map(|tz| tz.to_chrono()).transpose()?;
            let unit = Unit::from_polars(*unit);
            atoms(
                s.datetime()?
                    .physical()
                    .iter()
                    .map(|t| temporal::format_timestamp(t?, unit, zone.as_ref())),
            )
        }
        DataType::Duration(unit) => {
            let unit = Unit::from_polars(*unit);
            atoms(
                s.duration()?
                    .physical()
                    .iter()
                    .map(|d| Some(temporal::format_duration(d?, unit))),
            )
        }
        DataType::Time => atoms(
            s.time()?
                .physical()
                .iter()
                .map(|t| Some(temporal::format_time(t?))),
        ),
        DataType::Array(inner, _) => encode(&s.cast(&DataType::List(inner.clone()))?)?,
        DataType::List(_) => {
            let ca = s.list()?.rechunk();
            let elements = encode(&ca.get_inner())?;
            let arr = ca.downcast_as_array();
            (0..arr.len())
                .map(|row| {
                    arr.is_valid(row).then(|| {
                        let (start, end) = arr.offsets().start_end(row);
                        Sexp::List(
                            elements[start..end]
                                .iter()
                                .map(|e| e.clone().unwrap_or_else(nil))
                                .collect(),
                        )
                    })
                })
                .collect()
        }
        DataType::Struct(_) => {
            let ca = s.struct_()?;
            let fields = ca.fields_as_series();
            let encoded_fields = fields
                .iter()
                .map(encode)
                .collect::<PolarsResult<Vec<_>>>()?;
            let is_null = s.is_null();
            (0..s.len())
                .map(|row| {
                    if is_null.get(row) == Some(true) {
                        return None;
                    }
                    let entries: Vec<Sexp> = fields
                        .iter()
                        .zip(&encoded_fields)
                        .filter_map(|(field, values)| {
                            Some(entry(field.name(), values[row].clone()?))
                        })
                        .collect();
                    Some(match (entries.is_empty(), fields.first()) {
                        (true, Some(first)) => Sexp::List(vec![entry(first.name(), nil())]),
                        _ => Sexp::List(entries),
                    })
                })
                .collect()
        }
        dtype
            if dtype.is_primitive_numeric()
                || dtype.is_bool()
                || dtype.is_decimal()
                || dtype.is_categorical()
                || dtype.is_enum() =>
        {
            let text = s.cast(&DataType::String)?;
            atoms(text.str()?.iter().map(|v| v.map(str::to_string)))
        }
        dtype => polars_bail!(InvalidOperation: "sexp.encode does not support {}", dtype),
    })
}
