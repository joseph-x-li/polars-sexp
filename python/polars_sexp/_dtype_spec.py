from __future__ import annotations

from typing import TYPE_CHECKING, Any

import polars as pl

if TYPE_CHECKING:
    from polars._typing import PolarsDataType

_SCALAR_KINDS = [
    (pl.String, "string"),
    (pl.Boolean, "boolean"),
    (pl.Int8, "int8"),
    (pl.Int16, "int16"),
    (pl.Int32, "int32"),
    (pl.Int64, "int64"),
    (pl.Int128, "int128"),
    (pl.UInt8, "uint8"),
    (pl.UInt16, "uint16"),
    (pl.UInt32, "uint32"),
    (pl.UInt64, "uint64"),
    (pl.UInt128, "uint128"),
    (pl.Float32, "float32"),
    (pl.Float64, "float64"),
]

_DECODED_AS_TEXT_THEN_CAST = (pl.Enum, pl.Categorical, pl.Decimal)


def _with_default_time_unit(dtype: PolarsDataType) -> PolarsDataType:
    if isinstance(dtype, type) and dtype in (pl.Datetime, pl.Duration):
        return dtype()
    return dtype


def _is_text_then_cast(dtype: PolarsDataType) -> bool:
    return isinstance(dtype, _DECODED_AS_TEXT_THEN_CAST) or dtype in _DECODED_AS_TEXT_THEN_CAST


def needs_polars_cast(dtype: PolarsDataType) -> bool:
    if _is_text_then_cast(dtype):
        return True
    if isinstance(dtype, (pl.List, pl.Array)):
        return needs_polars_cast(dtype.inner)
    if isinstance(dtype, pl.Struct):
        return any(needs_polars_cast(f.dtype) for f in dtype.fields)
    return False


def dtype_spec(dtype: PolarsDataType, format: str | None) -> dict[str, Any]:
    dtype = _with_default_time_unit(dtype)
    if _is_text_then_cast(dtype):
        return {"kind": "string"}
    for scalar, kind in _SCALAR_KINDS:
        if dtype == scalar:
            return {"kind": kind}
    if dtype == pl.Date:
        return {"kind": "date", "format": format}
    if dtype == pl.Time:
        return {"kind": "time", "format": format}
    if isinstance(dtype, pl.Datetime):
        return {
            "kind": "datetime",
            "unit": dtype.time_unit,
            "time_zone": dtype.time_zone,
            "format": format,
        }
    if isinstance(dtype, pl.Duration):
        return {"kind": "duration", "unit": dtype.time_unit}
    if isinstance(dtype, pl.List):
        return {"kind": "list", "inner": dtype_spec(dtype.inner, format)}
    if isinstance(dtype, pl.Array):
        return {"kind": "array", "inner": dtype_spec(dtype.inner, format), "width": dtype.size}
    if isinstance(dtype, pl.Struct):
        return {
            "kind": "struct",
            "fields": [
                {"name": f.name, "dtype": dtype_spec(f.dtype, format)} for f in dtype.fields
            ],
        }
    msg = f"decode() does not support dtype {dtype}"
    raise TypeError(msg)


def dtype_from_spec(spec: dict[str, Any]) -> PolarsDataType:
    kind = spec["kind"]
    for scalar, scalar_kind in _SCALAR_KINDS:
        if kind == scalar_kind:
            return scalar()
    if kind == "date":
        return pl.Date()
    if kind == "datetime":
        return pl.Datetime(spec["unit"], spec["time_zone"])
    if kind == "list":
        return pl.List(dtype_from_spec(spec["inner"]))
    if kind == "struct":
        return pl.Struct({f["name"]: dtype_from_spec(f["dtype"]) for f in spec["fields"]})
    msg = f"unexpected inferred kind {kind!r}"
    raise ValueError(msg)
