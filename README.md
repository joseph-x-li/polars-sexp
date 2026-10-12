# polars-sexp

A [Polars](https://pola.rs) expression plugin, written in Rust, for parsing S-expressions.

```python
import polars as pl
import polars_sexp  # registers the `.sexp` namespace

df = pl.DataFrame({"s": ["(add 1 (mul 2 3))", '(say "hi")', "(broken"]})
df.with_columns(
    json=pl.col("s").sexp.to_json(),
    ok=pl.col("s").sexp.is_valid(),
)
# ┌───────────────────┬─────────────────────────────┬───────┐
# │ s                 ┆ json                        ┆ ok    │
# ╞═══════════════════╪═════════════════════════════╪═══════╡
# │ (add 1 (mul 2 3)) ┆ ["add","1",["mul","2","3"]] ┆ true  │
# │ (say "hi")        ┆ ["say","hi"]                ┆ true  │
# │ (broken           ┆ null                        ┆ false │
# └───────────────────┴─────────────────────────────┴───────┘
```

## Functions

Every function is available as `polars_sexp.<name>(expr, ...)` and as `pl.col(...).sexp.<name>(...)`.
Missing, invalid or non-matching values become null.

| Expression | Output | What it does |
|---|---|---|
| `.sexp.decode(dtype)` | `dtype` | Parse straight into a Polars type (see below). |
| `.sexp.get(*path)` | `String` | Look up a field in record-style sexps like `((name foo) (price 3))`. Each key steps one record deeper. |
| `.sexp.to_json()` | `String` | Lists become JSON arrays, atoms become JSON strings. A fallback for irregular data. |
| `.sexp.is_valid()` | `Boolean` | True when the string is exactly one well-formed S-expression. |

`decode` supports:

- `pl.String`: atoms as-is, lists as S-expression text.
- `pl.Boolean`: the atoms `true` and `false`.
- Integer and float types: atoms parsed as numbers.
- `pl.List(inner)`: the elements of a list, each decoded as `inner`.
- `pl.Struct(...)`: a record `((field value) ...)`. A field with several values, like `(tags a b)`, is treated as a list.

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
```

Syntax supported: `( … )` lists, bare symbols, `"double quoted"` strings with `\n \t \r \" \\` escapes, and `;` line comments.

## Development

Requires Rust (stable) and Python ≥ 3.9.

```sh
python -m venv .venv && source .venv/bin/activate
pip install maturin polars pytest
maturin develop --release   # builds the Rust crate into the venv
pytest                      # Python tests
cargo test                  # Rust parser unit tests
```

Layout:

- `src/sexp.rs`: the parser (pure Rust, no Polars).
- `src/decode.rs`: decoding parsed sexps into Polars dtypes.
- `src/expressions.rs`: `#[polars_expr]` functions that Polars calls.
- `python/polars_sexp/__init__.py`: Python wrappers and the `.sexp` namespace.
