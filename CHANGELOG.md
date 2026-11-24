# Changelog

## 0.3.1 - 2026-11-24

- Automate coordinated publication of the derive crate and parser after CI and conformance pass.

## 0.3.0 - 2026-11-16

### Changed
- `unsafe` is confined to the internal `src/raw/` module (arena, arena
  vector, item payload union and layout casts, owned error paths, shallow
  flatten copies, ASCII text fast path). The crate root is
  `#![deny(unsafe_code)]` with `#[allow(unsafe_code)]` only on `mod raw`;
  `scripts/check-unsafe.sh` enforces this in CI. Every function in `raw`
  that is not `unsafe fn` is sound for any arguments.
- `unsafe` occurrences: about 165 across 14 files in 0.2.0, 87 in 6 files
  under `src/raw/` in 0.3.0. Parser fast paths, emit prefixes, partitioning,
  normalization, time parsing, error formatting and serde helpers are now
  safe code.
- `rs-toml-macros` is `#![forbid(unsafe_code)]`.
- Performance against 0.2.0 (criterion, paired runs on a shared machine):
  parsing 0-44% faster depending on run; emit, serde and derive benchmarks
  within run-to-run noise (a per-commit bisect found no step slower than
  0.2.0).

### Fixed
- Error path construction checks its size before narrowing to `u32`.

## 0.2.0 - 2026-11-16

### Added
- serde `Deserialize` support behind the `serde` feature: `rs_toml::serde`
  with `from_str`, `from_document`, `from_item`, `ItemDeserializer` and a
  spanned `Error` implementing `serde::de::Error`. It reads the tree built by
  the existing parser and supports `deny_unknown_fields`, defaults, `Option`,
  internally/externally/adjacently tagged and untagged enums, `flatten` and
  `deserialize_with`. Integers are range checked, never wrapped; datetimes
  deserialize as strings.

### Fixed
- The nesting limit is now one cumulative budget for the whole tree.
  Previously table headers, dotted keys and inline values each had their
  own 256-level allowance, so `[[a.a...]]` headers combined with dotted
  keys and nested arrays could build a tree several hundred levels deep
  and overflow the stack in later recursive walks (contrary to the 0.1.1
  note). Every container now counts toward a single limit, measured from
  the root, and an array of tables counts as two levels (the array and its
  entry).

- Unknown-variant errors from `from_str`/`FromToml` no longer quote the
  offending value (it may be a secret); the span still locates it.

### Changed
- The nesting limit is lowered from 256 to 128 levels, so emit, `Debug`,
  `Clone`, `FromToml` and serde (including untagged recursive enums) walk
  any parsed tree within a 512 KiB stack in a debug build. Deeper input
  fails with "Max recursion depth exceeded" and a span.
- `rs_toml::serde::ItemDeserializer` returns an error instead of
  recursing past the same limit, which guards trees built or edited
  through the mutable API.
- `rs_toml::serde::Error` messages never contain values from the document:
  invalid types and values are described by kind (`invalid type: string,
  expected u16`) and unknown enum variants are not quoted (`unknown
  variant, expected `a` or `b``), because values may be secrets. Unknown
  field names (table keys) are still quoted. Messages passed to
  `Error::custom` by user `Deserialize` code are kept as written.
- Integers beyond the 64-bit range deserialize into `i128`/`u128` fields;
  where serde buffers input (`flatten`, untagged and internally tagged
  enums, self-describing visitors) they fail with "integer out of 64-bit
  range is not supported here" instead of a confusing type error.

## 0.1.1 — 2026-11-15

### Fixed
- Dotted keys (`a.b.c = 1`) and table headers (`[a.b.c]`, `[[a.b.c]]`) now
  count toward the 256-level nesting limit. Previously they could build an
  arbitrarily deep tree that overflowed the stack when it was later walked
  (for example by `Debug`, emit or clone). Such input now fails with
  "Max recursion depth exceeded".
- The derive macro no longer resolves `Some`, `None`, `Ok` or `Err` through
  the caller's scope, so a glob import of same-named items no longer breaks it.
- `Table::try_with_capacity` and `Array::try_with_capacity` return `None`
  above the new `MAX_CAPACITY` (16 Mi entries) instead of aborting on
  allocation failure.
- The "file too large" error now states the real limit, 256 MiB.
- Internal: `normalize()` returns a reference tied to its borrow; SAFETY
  comments added to the two `unsafe impl`s in the macros crate.

### Changed
- Minimum Rust version is 1.99.1. Dependencies updated to their latest
  releases.

## 0.1.0

- Initial version, split from `codixor/rust-misc`.
