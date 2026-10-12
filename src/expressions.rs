use polars::prelude::*;
use pyo3_polars::derive::polars_expr;

use crate::sexp;

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
