from datetime import date, datetime, time, timedelta
from decimal import Decimal

import polars as pl
import polars_sexp
import pytest


def test_invalid_input_is_null_in_every_function():
    df = pl.DataFrame({"s": ["(broken", "((a 1)) trailing", None]})
    out = df.select(
        json=pl.col("s").sexp.to_json(),
        valid=pl.col("s").sexp.is_valid(),
        got=pl.col("s").sexp.get("a"),
        decoded=pl.col("s").sexp.decode(pl.List(pl.String)),
    )
    assert out.to_dict(as_series=False) == {
        "json": [None, None, None],
        "valid": [False, False, None],
        "got": [None, None, None],
        "decoded": [None, None, None],
    }


def test_values_not_matching_the_dtype_become_null_without_failing_the_row():
    out = pl.select(
        ints=pl.lit("(4 x 300 -129)").sexp.decode(pl.List(pl.Int8)),
        flag=pl.lit("yes").sexp.decode(pl.Boolean),
    )
    assert out.to_dict(as_series=False) == {"ints": [[4, None, None, None]], "flag": [None]}


def test_only_atoms_decode_to_a_null_struct():
    dtype = pl.Struct({"a": pl.String})
    df = pl.DataFrame({"s": ["atom", "(not a record)"]})
    assert df.select(pl.col("s").sexp.decode(dtype))["s"].to_list() == [None, {"a": None}]


def test_format_applies_to_every_date_in_the_dtype():
    dtype = pl.Struct({"start": pl.Date, "end": pl.Date})
    out = pl.select(
        pl.lit("((start 01/02/2026) (end 2026-02-03))").sexp.decode(dtype, format="%d/%m/%Y")
    ).item()
    assert out == {"start": date(2026, 2, 1), "end": None}


def test_bare_temporal_classes_use_polars_default_time_unit():
    out = pl.select(
        t=pl.lit("2026-10-12").sexp.decode(pl.Datetime),
        d=pl.lit("1s").sexp.decode(pl.Duration),
    )
    assert out.schema == pl.Schema({"t": pl.Datetime("us"), "d": pl.Duration("us")})


def test_a_field_with_one_atom_decodes_as_a_one_element_list():
    dtype = pl.Struct({"tags": pl.List(pl.String)})
    df = pl.DataFrame({"s": ["((tags red fruit))", "((tags red))", "((tags (red)))", "((tags))"]})
    assert df.select(pl.col("s").sexp.decode(dtype))["s"].to_list() == [
        {"tags": ["red", "fruit"]},
        {"tags": ["red"]},
        {"tags": ["red"]},
        {"tags": []},
    ]


def test_one_format_serves_both_date_and_datetime_fields():
    dtype = pl.Struct({"day": pl.Date, "at": pl.Datetime("ms")})
    out = pl.select(
        dated=pl.lit("((day 01/02/2026) (at 01/02/2026))").sexp.decode(dtype, format="%d/%m/%Y"),
        timed=pl.lit('((day "01/02/2026 10:30") (at "01/02/2026 10:30"))').sexp.decode(
            dtype, format="%d/%m/%Y %H:%M"
        ),
    )
    assert out.row(0) == (
        {"day": date(2026, 2, 1), "at": datetime(2026, 2, 1)},
        {"day": date(2026, 2, 1), "at": datetime(2026, 2, 1, 10, 30)},
    )


def test_array_rows_of_the_wrong_width_are_null():
    df = pl.DataFrame({"s": ["(1 2 3)", "(1 2)", "((1 2 3) (4 5 6))"]})
    assert df.select(pl.col("s").sexp.decode(pl.Array(pl.Int8, 3)))["s"].to_list() == [
        [1, 2, 3],
        None,
        None,
    ]
    assert df.select(pl.col("s").sexp.decode(pl.Array(pl.Int8, (2, 3))))["s"].to_list() == [
        None,
        [None, None],
        [[1, 2, 3], [4, 5, 6]],
    ]


def test_encode_then_decode_round_trips_every_dtype_with_nulls_at_every_level():
    df = pl.DataFrame(
        {
            "text": ["has space", "", None],
            "tags": [["a"], [None, "b"], None],
            "points": [[{"x": 1}, None], [], None],
            "blank": [{"w": None}, None, {"w": 1}],
            "dims": [{"w": 1.5, "h": None}, None, {"w": float("nan"), "h": 1e300}],
            "at": [datetime(1969, 12, 31, 23, 59, 59, 1), datetime(2026, 1, 1), None],
            "took": [timedelta(microseconds=-1), None, timedelta(days=2)],
            "clock": [time(0, 0, 0, 500), None, time(23, 59)],
            "day": [date(1900, 1, 1), None, date(2026, 10, 12)],
            "flag": [True, None, False],
        }
    ).with_columns(
        zoned=pl.col("at").dt.replace_time_zone("Asia/Kolkata"),
        local_mean_time=pl.Series([datetime(1890, 1, 1, 12), None, None]).dt.replace_time_zone(
            "Europe/Paris"
        ),
        price=pl.Series([Decimal("1.50"), None, Decimal("-3.25")], dtype=pl.Decimal(10, 2)),
        size=pl.Series(["s", "m", None], dtype=pl.Enum(["s", "m"])),
        huge=pl.Series([2**100, None, -1], dtype=pl.Int128),
        pair=pl.Series([[1, None], None, [3, 4]], dtype=pl.Array(pl.Int8, 2)),
        clocks=pl.Series([[time(1, 2, 3, 4)], None, []], dtype=pl.List(pl.Time)),
        costs=pl.Series([[Decimal("1.25"), None], [], None], dtype=pl.List(pl.Decimal(5, 2))),
    )
    encoded = df.select(s=pl.struct(pl.all()).sexp.encode())
    decoded = encoded.select(pl.col("s").sexp.decode(pl.Struct(df.schema))).unnest("s")
    assert decoded.equals(df)


def test_tagged_payloads_can_be_positional_or_keyed():
    variants = {"circle": pl.Float64, "rect": pl.Struct({"w": pl.Int8, "h": pl.Int8})}
    shapes = pl.Series(["(circle 3)", "(rect 2 4)", "(rect (h 4))", "(rect (2 4))", "(blob 1)"])
    assert shapes.sexp.decode_tagged(variants).to_list() == [
        {"tag": "circle", "circle": 3.0, "rect": None},
        {"tag": "rect", "circle": None, "rect": {"w": 2, "h": 4}},
        {"tag": "rect", "circle": None, "rect": {"w": None, "h": 4}},
        {"tag": "rect", "circle": None, "rect": {"w": None, "h": None}},
        None,
    ]


def test_strict_raises_on_mismatch_but_not_on_missing_fields():
    dtype = pl.Struct({"a": pl.Int8, "b": pl.Int8})
    assert pl.Series(["((a 1))"]).sexp.decode(dtype, strict=True).to_list() == [{"a": 1, "b": None}]
    with pytest.raises(pl.exceptions.ComputeError, match="300"):
        pl.Series(["((a 300))"]).sexp.decode(dtype, strict=True)
    with pytest.raises(pl.exceptions.ComputeError, match="unclosed"):
        pl.Series(["((a 1)"]).sexp.decode(dtype, strict=True)


def test_inference_reads_empty_lists_as_null_and_one_atom_fields_as_lists():
    rows = pl.Series(["((n (1 ())) (tags a))", "((tags a b) (n ()))", "((huge 99999999999999999999))"])
    assert rows.sexp.infer_dtype() == pl.Struct(
        {"n": pl.List(pl.Int64), "tags": pl.List(pl.String), "huge": pl.Int128}
    )
