# Changelog

## Unreleased

- `decode` works through the column in batches of 16,384 rows, so memory stays flat instead of growing with the column. Decoding 1M records (64 MB of text) peaked at 35 MB instead of 1.17 GB, and ran about twice as fast.
- `encode` of a struct whose fields are all null now decodes back to those nulls when the first field is a list. It used to come back as an empty list.
- Inference reads a single atom as a one-item list only inside a record field. Rows like `a` and `(a b)` now infer `String` instead of `List(String)`, which strict decode rejected.
- `s.sexp.encode()` and `decode` work with zero-width `pl.Array` columns, at any depth. Before, they crashed the plugin. The `pl.col(...).sexp.encode()` form still fails on them until the Polars Rust crate ships the upstream fix.
- `Cargo.toml` pins pyo3 to 0.29.2, so a `cargo update` can't bring back the macOS segfault.

## 0.1.1 (2026-10-23)

- Fix a segfault on import on macOS. pyo3 0.29.3 made the Polars allocator recurse until the stack overflowed ([pola-rs/polars#29731](https://github.com/pola-rs/polars/issues/29731)), so the lockfile pins pyo3 0.29.2.
- Require Python 3.10 or newer, matching Polars 2.0. Wheels are now built for `abi3-py310`.
- Shorter README with an install line.
- CI tests Python 3.10 and 3.13 on Linux, macOS and Windows, against the oldest, newest and pre-release Polars, and runs weekly.
- CI checks `cargo fmt`, `clippy` and `ruff`.
- The release workflow installs and tests every wheel before publishing, and also runs (without publishing) on pull requests that change it.

## 0.1.0 (2026-10-16)

First release.

- `sexp.decode(dtype)` parses S-expressions into strings, booleans, integers up to 128 bits, floats, decimals, enums, categoricals, lists, arrays, structs, dates, datetimes, times and durations, with an optional `format` for temporal values.
- `sexp.decode()` on a Series infers the dtype, and `polars_sexp.infer_dtype` returns it.
- `sexp.decode_tagged(variants)` decodes tagged values like `(circle 3)` into a struct with an Enum `tag`.
- `sexp.encode()` writes any column `decode` reads back as S-expression text.
- `sexp.get(key, *nested_keys)`, `sexp.to_json()` and `sexp.is_valid()`.
- `strict=True` raises on invalid input instead of returning null.
- Wheels for Linux (glibc and musl), macOS and Windows on x86_64 and aarch64.
