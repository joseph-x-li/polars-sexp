from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Any

import polars as pl
from polars.plugins import register_plugin_function

from polars_sexp._internal import __version__

if TYPE_CHECKING:
    from polars._typing import IntoExpr, PolarsDataType

__all__ = ["__version__", "decode", "get", "is_valid", "to_json"]

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


def get(expr: IntoExpr, *path: str) -> pl.Expr:
    """Look up a field in record-style S-expressions like ``((name foo) (price 3))``.

    Each key in ``path`` steps one level into a nested record, so
    ``get("s", "inner", "x")`` reads ``((inner ((x 1))))`` as ``"1"``.
    A field with one value returns it; a field with several, like
    ``(tags a b)``, returns them as the list ``"(a b)"``. Atoms come back as
    their text and lists as S-expression text. Missing keys and invalid
    input become null.
    """
    if not path:
        msg = "get() needs at least one key"
        raise ValueError(msg)
    return register_plugin_function(
        plugin_path=PLUGIN_PATH,
        function_name="sexp_get",
        args=expr,
        kwargs={"path": list(path)},
        is_elementwise=True,
    )


def decode(expr: IntoExpr, dtype: PolarsDataType) -> pl.Expr:
    """Parse each string as one S-expression and decode it into ``dtype``.

    - ``pl.String``: atoms as-is, lists as S-expression text.
    - ``pl.Boolean``: the atoms ``true`` and ``false``.
    - Integer and float types: atoms parsed as numbers.
    - ``pl.List(inner)``: the elements of a list, each decoded as ``inner``.
    - ``pl.Struct(fields)``: a record ``((field value) ...)``, looked up by
      field name like :func:`get`.

    Anything missing, invalid or not matching the dtype becomes null.
    """
    return register_plugin_function(
        plugin_path=PLUGIN_PATH,
        function_name="sexp_decode",
        args=expr,
        kwargs={"dtype": _dtype_spec(dtype)},
        is_elementwise=True,
    )


_INTS = {
    pl.Int8: (8, True),
    pl.Int16: (16, True),
    pl.Int32: (32, True),
    pl.Int64: (64, True),
    pl.UInt8: (8, False),
    pl.UInt16: (16, False),
    pl.UInt32: (32, False),
    pl.UInt64: (64, False),
}


def _dtype_spec(dtype: PolarsDataType) -> dict[str, Any]:
    """Describe a Polars dtype in the form the Rust side deserializes."""
    if dtype == pl.String:
        return {"kind": "string"}
    if dtype == pl.Boolean:
        return {"kind": "boolean"}
    for int_type, (bits, signed) in _INTS.items():
        if dtype == int_type:
            return {"kind": "int", "bits": bits, "signed": signed}
    if dtype == pl.Float32:
        return {"kind": "float", "bits": 32}
    if dtype == pl.Float64:
        return {"kind": "float", "bits": 64}
    if isinstance(dtype, pl.List):
        return {"kind": "list", "inner": _dtype_spec(dtype.inner)}
    if isinstance(dtype, pl.Struct):
        return {
            "kind": "struct",
            "fields": [{"name": f.name, "dtype": _dtype_spec(f.dtype)} for f in dtype.fields],
        }
    msg = f"decode() does not support dtype {dtype}"
    raise TypeError(msg)


@pl.api.register_expr_namespace("sexp")
class SexpNamespace:
    """``pl.col("x").sexp.to_json()`` and friends."""

    def __init__(self, expr: pl.Expr) -> None:
        self._expr = expr

    def to_json(self) -> pl.Expr:
        return to_json(self._expr)

    def is_valid(self) -> pl.Expr:
        return is_valid(self._expr)

    def get(self, *path: str) -> pl.Expr:
        return get(self._expr, *path)

    def decode(self, dtype: PolarsDataType) -> pl.Expr:
        return decode(self._expr, dtype)
