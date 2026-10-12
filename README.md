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
| `.sexp.decode(dtype, format=None)` | `dtype` | Parse straight into a Polars type (see below). |
| `.sexp.get(key, *nested_keys)` | `String` | Look up a field in record-style sexps like `((name foo) (price 3))`. Each key steps one record deeper. |
| `.sexp.to_json()` | `String` | Lists become JSON arrays, atoms become JSON strings. A fallback for irregular data. |
| `.sexp.is_valid()` | `Boolean` | True when the string is exactly one well-formed S-expression. |

`decode` supports:

- `pl.String`: atoms as-is, lists as S-expression text.
- `pl.Boolean`: the atoms `true` and `false`.
- Integer and float types: atoms parsed as numbers.
- `pl.List(inner)`: the elements of a list, each decoded as `inner`.
- `pl.Array(inner, shape)`: like `pl.List`, but a list of the wrong length is null.
- `pl.Struct(...)`: a record `((field value) ...)`. A field with several values, like `(tags a b)`, is treated as a list, and so is a field with a single atom, like `(tags a)`.
- `pl.Date`: ISO `2026-10-12`.
- `pl.Datetime(unit, time_zone)`: ISO forms like `2026-10-12T21:56:14`, `"2026-10-12 21:56:14.5"` or a bare date.
  Values with an offset (`Z`, `+02:00`, `+02`) become the instant they name; values without one are wall-clock times in `time_zone`.
- `pl.Duration(unit)`: `1d2h30m`, `1.5s`, `-250ms` (units `w d h m s ms us ns`), or a bare integer in `unit`.

`format` is a chrono strftime pattern (like `"%d/%m/%Y"`) that replaces the ISO defaults for every Date and Datetime in `dtype`.
A date-only pattern decodes a Datetime at midnight, and a pattern with a time still decodes a Date.

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
Lists nested more than 1024 deep are treated as invalid.

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

- `src/sexp.rs`: the `Sexp` type and its canonical text form.
- `src/parse.rs`: the parser (pure Rust, no Polars).
- `src/record.rs`: record field lookup over `((key value) ...)`.
- `src/temporal.rs`: date, datetime and duration parsing.
- `src/decode.rs`: decoding parsed sexps into Polars dtypes.
- `src/expressions.rs`: `#[polars_expr]` functions that Polars calls.
- `python/polars_sexp/__init__.py`: Python wrappers and the `.sexp` namespace.
- `python/polars_sexp/_dtype_spec.py`: describes a Polars dtype for the Rust side.

The code has no comments: names and types carry intent, and each non-obvious edge case is a test.
