from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

import polars as pl
from polars.plugins import register_plugin_function

from polars_sexp._internal import __version__

if TYPE_CHECKING:
    from polars._typing import IntoExpr

__all__ = ["__version__", "is_valid", "to_json"]

PLUGIN_PATH = Path(__file__).parent


def to_json(expr: IntoExpr) -> pl.Expr:
    """Parse each string as one S-expression and encode it as JSON.

    Lists become JSON arrays and atoms become JSON strings, so
    ``(add 1 (mul 2 3))`` becomes ``["add","1",["mul","2","3"]]``.
    Strings that are not exactly one well-formed S-expression become null.
    """
    return register_plugin_function(
        plugin_path=PLUGIN_PATH,
        function_name="sexp_to_json",
        args=expr,
        is_elementwise=True,
    )


def is_valid(expr: IntoExpr) -> pl.Expr:
    """Return whether each string is exactly one well-formed S-expression."""
    return register_plugin_function(
        plugin_path=PLUGIN_PATH,
        function_name="sexp_is_valid",
        args=expr,
        is_elementwise=True,
    )


@pl.api.register_expr_namespace("sexp")
class SexpNamespace:
    """``pl.col("x").sexp.to_json()`` and friends."""

    def __init__(self, expr: pl.Expr) -> None:
        self._expr = expr

    def to_json(self) -> pl.Expr:
        return to_json(self._expr)

    def is_valid(self) -> pl.Expr:
        return is_valid(self._expr)
