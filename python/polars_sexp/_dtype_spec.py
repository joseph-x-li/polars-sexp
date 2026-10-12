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
    (pl.UInt8, "uint8"),
    (pl.UInt16, "uint16"),
    (pl.UInt32, "uint32"),
    (pl.UInt64, "uint64"),
    (pl.Float32, "float32"),
    (pl.Float64, "float64"),
]


def _with_default_time_unit(dtype: PolarsDataType) -> PolarsDataType:
    if isinstance(dtype, type) and dtype in (pl.Datetime, pl.Duration):
        return dtype()
    return dtype


def dtype_spec(dtype: PolarsDataType, format: str | None) -> dict[str, Any]:
    dtype = _with_default_time_unit(dtype)
    for scalar, kind in _SCALAR_KINDS:
        if dtype == scalar:
            return {"kind": kind}
    if dtype == pl.Date:
        return {"kind": "date", "format": format}
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
    if isinstance(dtype, pl.Struct):
        return {
            "kind": "struct",
            "fields": [{"name": f.name, "dtype": dtype_spec(f.dtype, format)} for f in dtype.fields],
        }
    msg = f"decode() does not support dtype {dtype}"
    raise TypeError(msg)
