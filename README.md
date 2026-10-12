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

Every function is available as `polars_sexp.<name>(expr, ...)`, as `pl.col(...).sexp.<name>(...)` and on a Series as `s.sexp.<name>(...)`.
Missing, invalid or non-matching values become null, unless `strict=True` is passed where offered.

| Expression | Output | What it does |
|---|---|---|
| `.sexp.decode(dtype, format=None, strict=False)` | `dtype` | Parse straight into a Polars type (see below). |
| `.sexp.decode_tagged(variants, format=None, strict=False)` | `Struct` | Decode tagged values like `(circle 3)` or `(rect 2 4)` (see below). |
| `.sexp.encode()` | `String` | Write any column `decode` can read back as S-expression text. |
| `.sexp.get(key, *nested_keys)` | `String` | Look up a field in record-style sexps like `((name foo) (price 3))`. Each key steps one record deeper. |
| `.sexp.to_json()` | `String` | Lists become JSON arrays, atoms become JSON strings. A fallback for irregular data. |
| `.sexp.is_valid()` | `Boolean` | True when the string is exactly one well-formed S-expression. |

On a Series, `s.sexp.decode()` without a dtype infers one from the first `infer_schema_length=100` rows; `polars_sexp.infer_dtype(s)` returns it.
Inference produces String, Boolean, Int64, Int128, Float64, Date, Datetime, List and Struct, falling back to String when rows disagree.

### decode

- `pl.String`: atoms as-is, lists as S-expression text.
- `pl.Boolean`: the atoms `true` and `false`.
- Integer types up to `Int128`/`UInt128`, float types and `pl.Decimal`: atoms parsed as numbers.
- `pl.Enum` and `pl.Categorical`: atoms as categories.
- `pl.List(inner)`: the elements of a list, each decoded as `inner`.
- `pl.Array(inner, shape)`: like `pl.List`, but a list of the wrong length is null.
- `pl.Struct(...)`: a record `((field value) ...)`. A field with several values, like `(tags a b)`, is treated as a list, and so is a field with a single atom, like `(tags a)`.
- `pl.Date`: ISO `2026-10-12`.
- `pl.Datetime(unit, time_zone)`: ISO forms like `2026-10-12T21:56:14`, `"2026-10-12 21:56:14.5"` or a bare date.
  Values with an offset (`Z`, `+02:00`, `+02`) become the instant they name; values without one are wall-clock times in `time_zone`.
- `pl.Duration(unit)`: `1d2h30m`, `1.5s`, `-250ms` (units `w d h m s ms us ns`), or a bare integer in `unit`.
- `pl.Time`: `21:56`, `21:56:14` or `21:56:14.5`.

The empty list `()` is null for every dtype except `pl.List` and `pl.Array`, where it is empty.

`format` is a chrono strftime pattern (like `"%d/%m/%Y"`) that replaces the ISO defaults for every Date, Datetime and Time in `dtype`.
A date-only pattern decodes a Datetime at midnight, and a pattern with a time still decodes a Date.

With `strict=True`, an invalid string or a value that does not fit its dtype raises instead of becoming null. Missing struct fields are still null.

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

### decode_tagged

`variants` maps each tag to the dtype of what follows it. The result has a `tag` field (an Enum of the variant names) and one field per variant, filled only on rows with that tag.
A Struct variant reads its fields by position, `(rect 2 4)`, or by name, `(rect (w 2) (h 4))`. Rows whose tag is not a variant are null.

```python
pl.col("s").sexp.decode_tagged({
    "circle": pl.Float64,
    "rect": pl.Struct({"w": pl.Float64, "h": pl.Float64}),
}).struct.unnest()
# s             tag     circle  rect
# (circle 3)    circle  3.0     null
# (rect 2 4)    rect    null    {2.0, 4.0}
# (rect (h 4))  rect    null    {null, 4.0}
```

### encode

Structs become records, lists and arrays become lists, and temporal values are written in the ISO forms `decode` reads, with offsets for zoned datetimes and unit suffixes for durations.
Null struct fields are left out and null list elements are written as `()`, so `decode` with the same dtype gives back the original column, except that a null element of a `List(List)` comes back as an empty list.

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
- `src/encode.rs`: writing Polars columns as sexps.
- `src/infer.rs`: inferring a dtype from parsed sexps.
- `src/expressions.rs`: `#[polars_expr]` functions that Polars calls.
- `python/polars_sexp/__init__.py`: Python wrappers and the `.sexp` namespace.
- `python/polars_sexp/_dtype_spec.py`: describes a Polars dtype for the Rust side.

The code has no comments: names and types carry intent, and each non-obvious edge case is a test.
