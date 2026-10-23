# polars-sexp

A [Polars](https://pola.rs) plugin, written in Rust, for parsing S-expressions.

```sh
pip install polars-sexp
```

```python
import polars as pl
import polars_sexp  # registers the `.sexp` namespace

df = pl.DataFrame({"s": ["(add 1 (mul 2 3))", '(say "hi")', "(broken"]})
df.with_columns(json=pl.col("s").sexp.to_json(), ok=pl.col("s").sexp.is_valid())
# ┌───────────────────┬─────────────────────────────┬───────┐
# │ s                 ┆ json                        ┆ ok    │
# ╞═══════════════════╪═════════════════════════════╪═══════╡
# │ (add 1 (mul 2 3)) ┆ ["add","1",["mul","2","3"]] ┆ true  │
# │ (say "hi")        ┆ ["say","hi"]                ┆ true  │
# │ (broken           ┆ null                        ┆ false │
# └───────────────────┴─────────────────────────────┴───────┘
```

## Functions

Each function works as `pl.col(...).sexp.<name>(...)`, `s.sexp.<name>(...)` on a Series, or `polars_sexp.<name>(expr, ...)`.
Invalid or non-matching values become null, unless `strict=True` is passed.

| Expression | Output | What it does |
|---|---|---|
| `.sexp.decode(dtype, format=None, strict=False)` | `dtype` | Parse into a Polars type. |
| `.sexp.decode_tagged(variants, format=None, strict=False)` | `Struct` | Decode tagged values like `(circle 3)` or `(rect 2 4)`. |
| `.sexp.encode()` | `String` | Write a column back as S-expression text that `decode` reads. |
| `.sexp.get(key, *nested_keys)` | `String` | Look up a field in records like `((name foo) (price 3))`. |
| `.sexp.to_json()` | `String` | Lists become JSON arrays, atoms JSON strings. |
| `.sexp.is_valid()` | `Boolean` | True for exactly one well-formed S-expression. |

`decode` handles strings, booleans, integers up to 128 bits, floats, decimals, enums and categoricals, lists, arrays, structs, and ISO dates, datetimes, times and durations (`1d2h30m`, `-250ms`).
A struct is read from a record `((field value) ...)`, and `format` takes a chrono pattern like `"%d/%m/%Y"` for temporal fields.
On a Series, `s.sexp.decode()` with no dtype infers one, and `polars_sexp.infer_dtype(s)` returns it.

```python
df = pl.DataFrame({"s": ["((name apple) (price 3) (tags red fruit) (dims ((w 1.5) (h 2))))"]})

df.select(pl.col("s").sexp.get("dims", "w"))   # "1.5"
df.select(pl.col("s").sexp.decode(pl.Struct({
    "name": pl.String,
    "price": pl.Int32,
    "tags": pl.List(pl.String),
    "dims": pl.Struct({"w": pl.Float64, "h": pl.Float64}),
})))
# {"name": "apple", "price": 3, "tags": ["red", "fruit"], "dims": {"w": 1.5, "h": 2.0}}

pl.col("s").sexp.decode_tagged({
    "circle": pl.Float64,
    "rect": pl.Struct({"w": pl.Float64, "h": pl.Float64}),
})
# a struct with a `tag` Enum plus one field per variant
```

Syntax: `( … )` lists, bare symbols, `"double quoted"` strings with `\n \t \r \" \\` escapes, and `;` line comments.

## Development

Requires Rust (stable) and Python ≥ 3.9.

```sh
python -m venv .venv && source .venv/bin/activate
pip install maturin polars pytest
maturin develop --release
pytest && cargo test
```
