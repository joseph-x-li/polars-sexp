from __future__ import annotations

import json
from pathlib import Path
from typing import TYPE_CHECKING

import polars as pl
from polars.plugins import register_plugin_function

from polars_sexp._dtype_spec import dtype_from_spec, dtype_spec, needs_polars_cast
from polars_sexp._internal import __version__

if TYPE_CHECKING:
    from collections.abc import Mapping

    from polars._typing import IntoExpr, PolarsDataType

__all__ = [
    "__version__",
    "decode",
    "decode_tagged",
    "encode",
    "get",
    "infer_dtype",
    "is_valid",
    "to_json",
]

PLUGIN_PATH = Path(__file__).parent
TAG_FIELD = "tag"
DEFAULT_INFER_SCHEMA_LENGTH = 100


def _call_plugin(function_name: str, expr: IntoExpr, **kwargs: object) -> pl.Expr:
    return register_plugin_function(
        plugin_path=PLUGIN_PATH,
        function_name=function_name,
        args=expr,
        kwargs=kwargs or None,
        is_elementwise=True,
    )


def to_json(expr: IntoExpr) -> pl.Expr:
    return _call_plugin("sexp_to_json", expr)


def is_valid(expr: IntoExpr) -> pl.Expr:
    return _call_plugin("sexp_is_valid", expr)


def get(expr: IntoExpr, key: str, *nested_keys: str) -> pl.Expr:
    return _call_plugin("sexp_get", expr, path=[key, *nested_keys])


def encode(expr: IntoExpr) -> pl.Expr:
    return _call_plugin("sexp_encode", expr)


def decode(
    expr: IntoExpr, dtype: PolarsDataType, *, format: str | None = None, strict: bool = False
) -> pl.Expr:
    decoded = _call_plugin(
        "sexp_decode", expr, dtype=dtype_spec(dtype, format), strict=strict
    )
    return decoded.cast(dtype, strict=strict) if needs_polars_cast(dtype) else decoded


def decode_tagged(
    expr: IntoExpr,
    variants: Mapping[str, PolarsDataType],
    *,
    format: str | None = None,
    strict: bool = False,
) -> pl.Expr:
    if not variants:
        msg = "decode_tagged() needs at least one variant"
        raise ValueError(msg)
    if TAG_FIELD in variants:
        msg = f"a variant cannot be named {TAG_FIELD!r}; that field holds the variant name"
        raise ValueError(msg)
    spec = {
        "kind": "tagged",
        "variants": [
            {"name": name, "dtype": dtype_spec(dtype, format)} for name, dtype in variants.items()
        ],
    }
    target = pl.Struct({TAG_FIELD: pl.Enum(list(variants)), **variants})
    return _call_plugin("sexp_decode", expr, dtype=spec, strict=strict).cast(
        target, strict=strict
    )


def infer_dtype(
    series: pl.Series, *, infer_schema_length: int | None = DEFAULT_INFER_SCHEMA_LENGTH
) -> PolarsDataType:
    sample = series if infer_schema_length is None else series.head(infer_schema_length)
    spec = sample.to_frame().select(
        register_plugin_function(
            plugin_path=PLUGIN_PATH,
            function_name="sexp_infer_dtype",
            args=pl.first(),
            returns_scalar=True,
        )
    )
    return dtype_from_spec(json.loads(spec.item()))


@pl.api.register_expr_namespace("sexp")
class SexpNamespace:
    def __init__(self, expr: pl.Expr) -> None:
        self._expr = expr

    def to_json(self) -> pl.Expr:
        return to_json(self._expr)

    def is_valid(self) -> pl.Expr:
        return is_valid(self._expr)

    def get(self, key: str, *nested_keys: str) -> pl.Expr:
        return get(self._expr, key, *nested_keys)

    def encode(self) -> pl.Expr:
        return encode(self._expr)

    def decode(
        self, dtype: PolarsDataType, *, format: str | None = None, strict: bool = False
    ) -> pl.Expr:
        return decode(self._expr, dtype, format=format, strict=strict)

    def decode_tagged(
        self,
        variants: Mapping[str, PolarsDataType],
        *,
        format: str | None = None,
        strict: bool = False,
    ) -> pl.Expr:
        return decode_tagged(self._expr, variants, format=format, strict=strict)


@pl.api.register_series_namespace("sexp")
class SexpSeriesNamespace:
    def __init__(self, series: pl.Series) -> None:
        self._series = series

    def _apply(self, expr: pl.Expr) -> pl.Series:
        return self._series.to_frame().select(expr).to_series()

    def to_json(self) -> pl.Series:
        return self._apply(to_json(pl.first()))

    def is_valid(self) -> pl.Series:
        return self._apply(is_valid(pl.first()))

    def get(self, key: str, *nested_keys: str) -> pl.Series:
        return self._apply(get(pl.first(), key, *nested_keys))

    def encode(self) -> pl.Series:
        return self._apply(encode(pl.first()))

    def infer_dtype(
        self, *, infer_schema_length: int | None = DEFAULT_INFER_SCHEMA_LENGTH
    ) -> PolarsDataType:
        return infer_dtype(self._series, infer_schema_length=infer_schema_length)

    def decode(
        self,
        dtype: PolarsDataType | None = None,
        *,
        format: str | None = None,
        strict: bool = False,
        infer_schema_length: int | None = DEFAULT_INFER_SCHEMA_LENGTH,
    ) -> pl.Series:
        if dtype is None:
            dtype = infer_dtype(self._series, infer_schema_length=infer_schema_length)
        return self._apply(decode(pl.first(), dtype, format=format, strict=strict))

    def decode_tagged(
        self,
        variants: Mapping[str, PolarsDataType],
        *,
        format: str | None = None,
        strict: bool = False,
    ) -> pl.Series:
        return self._apply(decode_tagged(pl.first(), variants, format=format, strict=strict))
