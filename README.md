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

| Expression | Output | Notes |
|---|---|---|
| `polars_sexp.to_json(expr)` / `.sexp.to_json()` | `String` | Lists become JSON arrays, atoms become JSON strings. Invalid input becomes null. Pair with `.str.json_decode(...)` to get native nested types. |
| `polars_sexp.is_valid(expr)` / `.sexp.is_valid()` | `Boolean` | True when the string is exactly one well-formed S-expression. |

Syntax supported: `( … )` lists, bare symbols, `"double quoted"` strings with `\n \t \r \" \\` escapes, and `;` line comments.
All atoms are kept as strings (no number or boolean coercion).

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
- `src/expressions.rs`: `#[polars_expr]` functions that Polars calls.
- `python/polars_sexp/__init__.py`: Python wrappers and the `.sexp` namespace.
