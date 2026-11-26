# rs-toml

`rs-toml` parses and emits TOML 1.1 in pure Rust. Its document model keeps
source spans so you can locate a value in the original text or preserve a
document's formatting when writing it back.

## Quick start

Version 0.4.1 requires Rust 1.100.0 or later. From your Cargo project, run

```sh
cargo add rs-toml@0.4.1
```

Put this in `src/main.rs` and run `cargo run`. The parsed document borrows
the source text and the arena.

```rust
use rs_toml::{Arena, parse};

fn main() -> Result<(), rs_toml::Error> {
    let arena = Arena::new();
    let doc = parse("[server]\nport = 8080", &arena)?;
    assert_eq!(doc["server"]["port"].as_i64(), Some(8080));
    Ok(())
}
```

Parsing, document access and emission work with the default feature set.
See the [API reference](https://docs.rs/rs-toml/0.4.1/rs_toml/) and
[release notes](./CHANGELOG.md).

## Public API

Core parse/document flow:

- [`parse`](./src/parser.rs)
- [`parse_recoverable`](./src/parser.rs)
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

### Optional features

Add `--features derive` or `--features serde` to the installation command.
To enable both, use `--features derive,serde`. The public conversion errors
use `thiserror`.

- `derive`
  - re-exports `#[derive(Toml)]` from [`rs-toml-macros`](./macros).
    The bare derive implements `FromToml`. Add `#[toml(Toml)]` to derive
    `ToToml` as well.
- `serde`
  - `Serialize` for `Item`, `Table`, `Document` and `Spanned` in
    [`impl_serde.rs`](./src/impl_serde.rs)
  - `Deserialize` support in [`serde.rs`](./src/serde.rs):
    `rs_toml::serde::from_str` (a drop-in for `toml::from_str`),
    `from_document`/`from_item` for borrowed results, and
    `rs_toml::serde::Error`, which carries the byte span of the offending key
    or value. Datetimes deserialize as RFC 3339 strings. The crate-level
    `rs_toml::from_str` stays the `FromToml` entry point.

## Bounded parsing and diagnostics

Use `parse_with(input, &arena, &limits)`, `parse_recoverable_with(input, &arena, &limits)`,
`from_str_with(input, &limits)` or `serde::from_str_with(input, &limits)` to apply
`rs_format::Limits`. With `Limits::default()` a document can contain up to 16 MiB,
each token body up to 1 MiB, and the tree up to 128 containers including the implicit
root table. Dotted keys and headers count their intermediate tables. Each array
of tables counts both the array and its entry. Token lengths are encoded bytes,
including escapes, comment whitespace and multiline content, excluding delimiters.
TOML has no expanding references and accepts a zero expansion budget.

Existing entry points retain the 0.3 limits of 129 containers including the root,
at most 256 MiB minus one byte, and no separate token limit. The nesting and
document ceilings also apply when a caller requests a larger budget.

The crate re-exports `FormatError`, `Category`, `Position` and `Limits`.
Parser and typed diagnostics retain their byte spans and report a zero-based
offset and one-based line and byte column when source text is available.
Detached items can report just an offset. Diagnostics discard custom visitor
messages and input characters. Error paths preserve key spans and array indices,
with a fixed placeholder for each key name.

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

From the repository root, run

- `bash ./scripts/validate.sh`
- `bash ./scripts/feature-matrix.sh`
- `bash ./scripts/bench.sh`

The validation flow covers:

- `cargo test`
- `cargo test --doc`
- `cargo test --examples`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo check --benches`

## Benchmarks

The committed Criterion entry point is:

- `cargo bench -p rs-toml --bench toml_core -- --sample-size 20`

The suite measures parsing, emission with default and preserved formatting,
and typed round-trips. The `serde` and `derive` features enable their
respective benchmarks. See [`toml_core.rs`](./benches/toml_core.rs).

Earlier runs of [`scripts/bench.sh`](./scripts/bench.sh) recorded these timings:

- `rs-toml/parse_small`: about `375-437 ns`
- `rs-toml/emit_small`: about `633-789 ns`
- `rs-toml/typed_round_trip`: about `2.30-2.87 µs`

Performance helpers stay opt-in and measurement-driven. The crate keeps the
simpler implementation unless a benchmarked change earns its complexity.

## KDL Interop

The `toml-interop` feature in `rs-kdl` provides a direct bridge between the
TOML and KDL document models and a generated TOML-on-KDL contract, following
the same pattern as its JSON and YAML adapters. See the
[`rs-kdl` repository](https://github.com/codixor/rs-kdl).

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

On main and version tags, CI runs the pinned nextest runner and checks API compatibility against the highest eligible non-yanked version of `rs-toml` on crates.io. On main, the published version matching the workspace version is eligible. For a tag, the baseline must be strictly older, including on a retry.
