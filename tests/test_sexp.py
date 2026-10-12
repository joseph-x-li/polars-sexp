import polars as pl
import polars_sexp as ps


def test_to_json():
    df = pl.DataFrame({"s": ["(add 1 (mul 2 3))", "atom", '(say "hi there")', "(broken", None]})
    out = df.select(ps.to_json("s"))["s"].to_list()
    assert out == [
        '["add","1",["mul","2","3"]]',
        '"atom"',
        '["say","hi there"]',
        None,
        None,
    ]


def test_namespace_and_is_valid():
    df = pl.DataFrame({"s": ["(a b)", "(a b", "a) b", None]})
    out = df.select(pl.col("s").sexp.is_valid())["s"].to_list()
    assert out == [True, False, False, None]


def test_json_decode_roundtrip():
    df = pl.DataFrame({"s": ["(a b c)", "(d e)"]})
    out = df.select(
        pl.col("s").sexp.to_json().str.json_decode(pl.List(pl.String))
    )["s"].to_list()
    assert out == [["a", "b", "c"], ["d", "e"]]


RECORDS = [
    "((name apple) (price 3) (tags red fruit) (dims ((w 1.5) (h 2))))",
    "((name pear) (price oops))",
    "(not a record)",
    "(broken",
    None,
]


def test_get():
    df = pl.DataFrame({"s": RECORDS})
    out = df.select(
        name=pl.col("s").sexp.get("name"),
        tags=pl.col("s").sexp.get("tags"),
        w=ps.get("s", "dims", "w"),
        dims=pl.col("s").sexp.get("dims"),
    )
    assert out.to_dict(as_series=False) == {
        "name": ["apple", "pear", None, None, None],
        "tags": ["(red fruit)", None, None, None, None],
        "w": ["1.5", None, None, None, None],
        "dims": ["((w 1.5) (h 2))", None, None, None, None],
    }


def test_decode_struct():
    dtype = pl.Struct(
        {
            "name": pl.String,
            "price": pl.Int32,
            "tags": pl.List(pl.String),
            "dims": pl.Struct({"w": pl.Float64, "h": pl.Float64}),
        }
    )
    df = pl.DataFrame({"s": RECORDS})
    out = df.select(pl.col("s").sexp.decode(dtype))
    assert out.schema["s"] == dtype
    assert out["s"].to_list() == [
        {"name": "apple", "price": 3, "tags": ["red", "fruit"], "dims": {"w": 1.5, "h": 2.0}},
        {"name": "pear", "price": None, "tags": None, "dims": None},
        {"name": None, "price": None, "tags": None, "dims": None},
        None,
        None,
    ]


def test_decode_lists_and_scalars():
    df = pl.DataFrame({"s": ["(1 2 3)", "()", "(4 x)", "7", None]})
    out = df.select(
        ints=pl.col("s").sexp.decode(pl.List(pl.Int64)),
        nested=pl.lit("((a b) (c))").sexp.decode(pl.List(pl.List(pl.String))),
        scalar=pl.col("s").sexp.decode(pl.UInt8),
        flag=pl.lit("true").sexp.decode(pl.Boolean),
    )
    assert out.to_dict(as_series=False) == {
        "ints": [[1, 2, 3], [], [4, None], None, None],
        "nested": [[["a", "b"], ["c"]]] * 5,
        "scalar": [None, None, None, 7, None],
        "flag": [True] * 5,
    }


def test_decode_lazy_schema():
    lf = pl.LazyFrame({"s": ["(1 2)"]}).select(pl.col("s").sexp.decode(pl.List(pl.Int16)))
    assert lf.collect_schema()["s"] == pl.List(pl.Int16)
    assert lf.collect()["s"].to_list() == [[1, 2]]


def test_decode_temporal():
    from datetime import date, datetime, timedelta, timezone

    dtype = pl.Struct(
        {
            "d": pl.Date,
            "t": pl.Datetime("ms"),
            "tz": pl.Datetime("us", "America/New_York"),
            "dur": pl.Duration,
        }
    )
    df = pl.DataFrame(
        {
            "s": [
                '((d 2026-10-12) (t "2026-10-12 21:56:14.5") (tz "2026-10-12 12:00:00Z") (dur 1h30m))',
                "((d 12/10/2026) (t 2026-10-12T21:56:14+02:00) (tz 2026-10-12T08:00) (dur 1.5s))",
                "((d nope) (t nope) (tz nope) (dur 5x))",
            ]
        }
    )
    out = df.select(pl.col("s").sexp.decode(dtype))
    assert out.schema["s"] == pl.Struct(
        {
            "d": pl.Date,
            "t": pl.Datetime("ms"),
            "tz": pl.Datetime("us", "America/New_York"),
            "dur": pl.Duration("us"),
        }
    )
    rows = out.unnest("s").with_columns(pl.col("tz").dt.convert_time_zone("UTC")).rows()
    utc = timezone.utc
    assert rows == [
        (
            date(2026, 10, 12),
            datetime(2026, 10, 12, 21, 56, 14, 500000),
            datetime(2026, 10, 12, 12, tzinfo=utc),
            timedelta(hours=1, minutes=30),
        ),
        (None, datetime(2026, 10, 12, 19, 56, 14), datetime(2026, 10, 12, 12, tzinfo=utc), timedelta(seconds=1.5)),
        (None, None, None, None),
    ]

    custom = pl.select(pl.lit("12/10/2026").sexp.decode(pl.Date, format="%d/%m/%Y")).item()
    assert custom == date(2026, 10, 12)
