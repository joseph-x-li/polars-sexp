# Changelog

## Unreleased

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
