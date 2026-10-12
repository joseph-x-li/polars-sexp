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
