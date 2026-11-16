# serde-deserialize: serde Deserialize support behind the `serde` feature
Level: L3 (public API)
Goal: `#[derive(serde::Deserialize)]` types load from TOML through rs-toml as a drop-in for `toml::from_str`.
Acceptance criteria:
- [ ] `rs_toml::serde::{from_str, from_document, from_item, ItemDeserializer, Error}` behind `serde`
- [ ] Errors implement `serde::de::Error` and carry the byte span of the offending key/value
- [ ] Integer range checks (no wrapping), Option, seq, map, struct, enums (unit, external, internal, untagged), newtype, unit, ignored_any, flatten
- [ ] tests/serde_de.rs covers each pattern, span correctness, and differential checks against the `toml` crate
- [ ] fmt, clippy (all targets/features, -D warnings), tests with all and no default features pass
Out of scope: Serializer (already exists), .github/, fuzz/, diver/, the vault repository
Likely files: src/serde.rs, src/lib.rs, src/impl_serde.rs (docs), Cargo.toml, tests/serde_de.rs, README.md, CHANGELOG.md
Risks: public API naming (`rs_toml::from_str` is taken by FromToml, so the serde entry points live in `rs_toml::serde`); no new `unsafe`; no second parser.

## Log
- Deserializer walks the parsed `Item` tree; datetimes are presented as RFC 3339 strings.
- Errors without a span (e.g. `missing field`, errors from serde's buffered content in
  internally tagged / untagged enums) take the span of the innermost enclosing value.
