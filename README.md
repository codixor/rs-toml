# rs-toml

`rs-toml` is a pure-Rust TOML parser, document/value model,
emitter, and optional derive crate.

The active crate shape is intentionally simple:

- parser, document traversal, value access, and emission are core
- `thiserror` backs the public conversion errors
- derive support is additive behind `derive`
- serde support is additive behind `serde`
- proof tests live under crate-root `tests/`
- examples, benches, and scripts follow the same layout as the `sys-*` crates

## Public API

Core parse/document flow:

- [`parse`](./src/lib.rs)
- [`parse_recoverable`](./src/lib.rs)
- [`Document`](./src/parser.rs)
- [`Item`](./src/item.rs)
- [`Table`](./src/item/table.rs)
- [`Array`](./src/item/array.rs)

Typed conversion and emission:

- [`from_str`](./src/lib.rs)
- [`to_string`](./src/lib.rs)
- [`Formatting`](./src/lib.rs)
- [`FromToml`](./src/de.rs)
- [`ToToml`](./src/ser.rs)
- [`FromTomlError`](./src/de.rs)
- [`ToTomlError`](./src/ser.rs)

Additive features:

- `derive`
  - re-exports `#[derive(Toml)]` from [`rs-toml/macros`](./macros)
- `serde`
  - `Serialize` for `Item`, `Table`, `Document` and `Spanned` in
    [`impl_serde.rs`](./src/impl_serde.rs)
  - `Deserialize` support in [`serde.rs`](./src/serde.rs):
    `rs_toml::serde::from_str` (a drop-in for `toml::from_str`),
    `from_document`/`from_item` for borrowed results, and
    `rs_toml::serde::Error`, which carries the byte span of the offending key
    or value. Datetimes deserialize as RFC 3339 strings. The crate-level
    `rs_toml::from_str` stays the `FromToml` entry point.

## Examples

- `cargo run -p rs-toml --example parse_document`
- `cargo run -p rs-toml --example emit_document`
- `cargo run -p rs-toml --example typed_round_trip`
- `cargo run -p rs-toml --example derive --features derive`

## Test Layout

The active proof layout is:

- public integration tests under [`tests/`](./tests)
- golden fixtures under [`tests/golden/`](./tests/golden)
- shared helpers under [`tests/support/`](./tests/support)
- private module-unit suites physically stored under [`tests/unit/`](./tests/unit)
  and wired back into internal modules with `#[path = ...]`

That keeps the crate tree clean without forcing private parser/emitter internals
into the public surface.

The crate-root golden surface now covers:

- basic parse and emit output
- preserved-format reprojection output
- temporal formatting stability
- span-to-source slice coverage

## Validation

- `bash ./rs-toml/scripts/validate.sh`
- `bash ./rs-toml/scripts/feature-matrix.sh`
- `bash ./rs-toml/scripts/bench.sh`

The validation flow covers:

- `cargo test`
- `cargo test --doc`
- `cargo test --examples`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo check --benches`

## Benchmarks

The committed Criterion entry point is:

- `cargo bench -p rs-toml --bench toml_core -- --sample-size 20`

Current bench coverage in [`toml_core.rs`](./benches/toml_core.rs):

- parse small document
- emit small document
- typed round-trip

Current baseline from `bash ./rs-toml/scripts/bench.sh`:

- `rs-toml/parse_small`: about `375-437 ns`
- `rs-toml/emit_small`: about `633-789 ns`
- `rs-toml/typed_round_trip`: about `2.30-2.87 µs`

Performance helpers stay opt-in and measurement-driven. The crate keeps the
simpler implementation unless a benchmarked change earns its complexity.

## KDL Interop

`rs-toml` itself stays focused on TOML parsing and emission. The additive KDL
adapter lives in [`rs-kdl`](../rs-kdl) and follows the same shape as the JSON and
YAML bridges:

- direct shared-model TOML bridge first
- generated TOML-on-KDL contract second

That keeps TOML ownership here, while `rs-kdl` stays the format adapter rather
than the semantic hub.

## Legacy Donor Material

The crate still carries a legacy [`diver/`](./diver) directory from the donor
history. It is no longer part of the active crate surface or validation flow.
The committed source of truth is the top-level crate layout in this directory.

That also applies to benchmarking and macro generation: the active sources of
truth are [`benches/toml_core.rs`](./benches/toml_core.rs),
[`scripts/bench.sh`](./scripts/bench.sh), and [`macros/`](./macros).

## License and origin

`rs-toml` is derived from [toml-spanner](https://github.com/exrok/toml-spanner)
(Copyright (c) 2019 Embark Studios, 2026 Thomas Dagenais). It is licensed, like
upstream, under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT](LICENSE-MIT) at your option. `diver/` holds upstream's benchmark and
toml-test tooling; it is not part of the workspace build.

## Releases

Bump the workspace version and the derive dependency together. Pushes to `main` run CI and conformance checks only. To publish, push a `v<version>` tag that matches the workspace version. After the tagged commit passes both checks, the `cratesio` environment publishes `rs-toml-macros` first, then `rs-toml`. The workflow creates the GitHub release for that tag. A retry skips any workspace member already published.
