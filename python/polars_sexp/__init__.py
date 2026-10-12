from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

import polars as pl
from polars.plugins import register_plugin_function

from polars_sexp._dtype_spec import dtype_spec
from polars_sexp._internal import __version__

if TYPE_CHECKING:
    from polars._typing import IntoExpr, PolarsDataType

__all__ = ["__version__", "decode", "get", "is_valid", "to_json"]

PLUGIN_PATH = Path(__file__).parent


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


def decode(expr: IntoExpr, dtype: PolarsDataType, *, format: str | None = None) -> pl.Expr:
    return _call_plugin("sexp_decode", expr, dtype=dtype_spec(dtype, format))


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

    def decode(self, dtype: PolarsDataType, *, format: str | None = None) -> pl.Expr:
        return decode(self._expr, dtype, format=format)
