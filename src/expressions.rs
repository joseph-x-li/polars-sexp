use polars::prelude::*;
use pyo3_polars::derive::polars_expr;
use serde::Deserialize;

use crate::decode::{self, DtypeSpec};
use crate::parse::parse_exactly_one;
use crate::record::Value;
use crate::sexp::Sexp;

fn parse_each_or_null(ca: &StringChunked) -> Vec<Option<Sexp>> {
    ca.iter()
        .map(|opt| opt.and_then(|v| parse_exactly_one(v).ok()))
        .collect()
}

#[polars_expr(output_type=String)]
fn sexp_to_json(inputs: &[Series]) -> PolarsResult<Series> {
    let ca = inputs[0].str()?;
    let out: StringChunked = parse_each_or_null(ca)
        .iter()
        .map(|parsed| Some(parsed.as_ref()?.to_json().to_string()))
        .collect();
    Ok(out.with_name(ca.name().clone()).into_series())
}

#[polars_expr(output_type=Boolean)]
fn sexp_is_valid(inputs: &[Series]) -> PolarsResult<Series> {
    let ca = inputs[0].str()?;
    let out: BooleanChunked =
        ca.apply_nonnull_values_generic(DataType::Boolean, |v| parse_exactly_one(v).is_ok());
    Ok(out.into_series())
}

#[derive(Deserialize)]
struct GetKwargs {
    path: Vec<String>,
}

#[polars_expr(output_type=String)]
fn sexp_get(inputs: &[Series], kwargs: GetKwargs) -> PolarsResult<Series> {
    let ca = inputs[0].str()?;
    let out: StringChunked = parse_each_or_null(ca)
        .iter()
        .map(|parsed| {
            let mut value = Value::Node(parsed.as_ref()?);
            for key in &kwargs.path {
                value = value.record_field(key)?;
            }
            Some(value.to_text_with_bare_atom())
        })
        .collect();
    Ok(out.with_name(ca.name().clone()).into_series())
}

#[derive(Deserialize)]
struct DecodeKwargs {
    dtype: DtypeSpec,
}

fn decode_output(input_fields: &[Field], kwargs: DecodeKwargs) -> PolarsResult<Field> {
    Ok(Field::new(
        input_fields[0].name().clone(),
        kwargs.dtype.dtype()?,
    ))
}

#[polars_expr(output_type_func_with_kwargs=decode_output)]
fn sexp_decode(inputs: &[Series], kwargs: DecodeKwargs) -> PolarsResult<Series> {
    let ca = inputs[0].str()?;
    let parsed = parse_each_or_null(ca);
    let vals: Vec<Option<Value>> = parsed.iter().map(|p| p.as_ref().map(Value::Node)).collect();
    decode::decode(ca.name().clone(), &vals, &kwargs.dtype)
}
