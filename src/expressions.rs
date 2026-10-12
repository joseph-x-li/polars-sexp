use polars::prelude::*;
use pyo3_polars::derive::polars_expr;
use serde::Deserialize;

use crate::decode::{Decoder, DtypeSpec};
use crate::encode::encode;
use crate::infer;
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
    strict: bool,
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
    let parsed: Vec<Option<Sexp>> = if kwargs.strict {
        ca.iter()
            .map(|opt| opt.map(parse_or_explain).transpose())
            .collect::<PolarsResult<_>>()?
    } else {
        parse_each_or_null(ca)
    };
    let vals: Vec<Option<Value>> = parsed.iter().map(|p| p.as_ref().map(Value::Node)).collect();
    let decoder = Decoder {
        strict: kwargs.strict,
    };
    decoder.decode(ca.name().clone(), &vals, &kwargs.dtype)
}

fn parse_or_explain(text: &str) -> PolarsResult<Sexp> {
    parse_exactly_one(text).map_err(|e| {
        polars_err!(ComputeError: "invalid S-expression at byte {}: {} in `{}`", e.pos, e.msg, text)
    })
}

#[polars_expr(output_type=String)]
fn sexp_encode(inputs: &[Series]) -> PolarsResult<Series> {
    let out: StringChunked = encode(&inputs[0])?
        .into_iter()
        .map(|sexp| sexp.map(|s| s.to_string()))
        .collect();
    Ok(out.with_name(inputs[0].name().clone()).into_series())
}

#[polars_expr(output_type=String)]
fn sexp_infer_dtype(inputs: &[Series]) -> PolarsResult<Series> {
    let ca = inputs[0].str()?;
    let shape = parse_each_or_null(ca)
        .iter()
        .flatten()
        .map(|sexp| infer::of_value(Value::Node(sexp)))
        .fold(infer::Shape::Unknown, infer::merge);
    let spec =
        serde_json::to_string(&shape.spec()).map_err(|e| polars_err!(ComputeError: "{}", e))?;
    Ok(StringChunked::from_slice(ca.name().clone(), &[spec]).into_series())
}
