# Changelog

## Unreleased

### Added
- serde `Deserialize` support behind the `serde` feature: `rs_toml::serde`
  with `from_str`, `from_document`, `from_item`, `ItemDeserializer` and a
  spanned `Error` implementing `serde::de::Error`. It reads the tree built by
  the existing parser and supports `deny_unknown_fields`, defaults, `Option`,
  internally/externally/adjacently tagged and untagged enums, `flatten` and
  `deserialize_with`. Integers are range checked, never wrapped; datetimes
  deserialize as strings.

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
