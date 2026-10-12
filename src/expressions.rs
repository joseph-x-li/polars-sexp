use polars::prelude::*;
use pyo3_polars::derive::polars_expr;
use serde::Deserialize;

use crate::decode::{self, Spec};
use crate::sexp::{self, Sexp, Value};

fn parse_all(ca: &StringChunked) -> Vec<Option<Sexp>> {
    ca.iter()
        .map(|opt| opt.and_then(|v| sexp::parse(v).ok()))
        .collect()
}

/// Parse each string as one S-expression and re-encode it as JSON
/// (lists -> arrays, atoms -> strings). Invalid input becomes null.
#[polars_expr(output_type=String)]
fn sexp_to_json(inputs: &[Series]) -> PolarsResult<Series> {
    let ca = inputs[0].str()?;
    let out: StringChunked = ca
        .iter()
        .map(|opt| {
            opt.and_then(|v| sexp::parse(v).ok())
                .map(|s| s.to_json().to_string())
        })
        .collect();
    Ok(out.with_name(ca.name().clone()).into_series())
}

/// Whether each string is exactly one well-formed S-expression.
#[polars_expr(output_type=Boolean)]
fn sexp_is_valid(inputs: &[Series]) -> PolarsResult<Series> {
    let ca = inputs[0].str()?;
    let out: BooleanChunked =
        ca.apply_nonnull_values_generic(DataType::Boolean, |v| sexp::parse(v).is_ok());
    Ok(out.into_series())
}

#[derive(Deserialize)]
struct GetKwargs {
    path: Vec<String>,
}

/// Follow `path` through nested records `((key value) ...)`. Atoms come back
/// as their text, lists as S-expression text; missing keys and invalid input are null.
#[polars_expr(output_type=String)]
fn sexp_get(inputs: &[Series], kwargs: GetKwargs) -> PolarsResult<Series> {
    let ca = inputs[0].str()?;
    let out: StringChunked = parse_all(ca)
        .iter()
        .map(|parsed| {
            let mut v = Value::One(parsed.as_ref()?);
            for key in &kwargs.path {
                v = v.field(key)?;
            }
            Some(v.to_text())
        })
        .collect();
    Ok(out.with_name(ca.name().clone()).into_series())
}

#[derive(Deserialize)]
struct DecodeKwargs {
    dtype: Spec,
}

fn decode_output(input_fields: &[Field], kwargs: DecodeKwargs) -> PolarsResult<Field> {
    Ok(Field::new(
        input_fields[0].name().clone(),
        kwargs.dtype.dtype()?,
    ))
}

/// Parse each string and decode it into the requested dtype; see `decode::decode`.
#[polars_expr(output_type_func_with_kwargs=decode_output)]
fn sexp_decode(inputs: &[Series], kwargs: DecodeKwargs) -> PolarsResult<Series> {
    let ca = inputs[0].str()?;
    let parsed = parse_all(ca);
    let vals: Vec<Option<Value>> = parsed.iter().map(|p| p.as_ref().map(Value::One)).collect();
    decode::decode(ca.name().clone(), &vals, &kwargs.dtype)
}
