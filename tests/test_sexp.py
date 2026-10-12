from datetime import date, datetime

import polars as pl
import polars_sexp


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
