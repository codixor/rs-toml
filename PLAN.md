# rs-toml Plan

Goal: keep `rs-toml` as the workspace's main pure-Rust TOML parser/emitter crate
with a smaller always-on core API, crate-root proof coverage, optional derive
support, and measured benchmark tooling.

## Direction

- [x] Keep the crate library-first.
- [x] Keep the core public API centered on:
  - [x] parse / document
  - [x] value tree
  - [x] emit / serialization
  - [x] typed errors
  - [x] additive derive macros
- [x] Simplify the public API so parser + emitter are always-on core features.
- [x] Stop presenting the main crate story as a donor-style `from-toml` /
  `to-toml` split for ordinary use.
- [x] Move tests under crate-root `tests/`.
- [x] Add examples, goldens, Criterion benches, and validation scripts.
- [x] Use `thiserror` for the public conversion errors.
- [x] Keep performance tooling measurement-driven.

## Stage 1: Naming, Workspace, And Manifest Cleanup

- [x] Align the crate to the root workspace model.
- [x] Remove the nested crate-local `[workspace]`.
- [x] Keep the crate identity as `rs-toml`.
- [x] Move the active proc-macro crate to [`macros/`](./macros).
- [x] Add and sync [`README.md`](./README.md).
- [x] Add and sync [`PLAN.md`](./PLAN.md).

## Stage 2: Error Model And Public API Cleanup

- [x] Keep the public API centered on parser/document/emitter usage.
- [x] Make parsing part of the always-on core API.
- [x] Make emission part of the always-on core API.
- [x] Keep derive additive behind `derive`.
- [x] Keep serde additive behind `serde`.
- [x] Convert public conversion errors to `thiserror`:
  - [x] [`FromTomlError`](./src/de.rs)
  - [x] [`ToTomlError`](./src/ser.rs)

## Stage 3: Parser, Document, And Emitter Surface

- [x] Audit the parse/document APIs:
  - [x] [`parser.rs`](./src/parser.rs)
  - [x] [`item.rs`](./src/item.rs)
  - [x] [`arena.rs`](./src/arena.rs)
  - [x] [`span.rs`](./src/span.rs)
  - [x] [`time.rs`](./src/time.rs)
- [x] Audit the emit/serialization APIs:
  - [x] [`emit.rs`](./src/emit.rs)
  - [x] [`ser.rs`](./src/ser.rs)
  - [x] [`emit/`](./src/emit)
- [x] Keep the public surface minimal and obvious:
  - [x] parse text
  - [x] inspect document/items
  - [x] emit text
  - [x] typed conversion in/out

## Stage 4: Derive Macro Reshape

- [x] Move active proc-macro support into [`rs-toml/macros/`](./macros).
- [x] Keep the public derive surface centered on `#[derive(Toml)]`.
- [x] Rename the macro crate identity to `rs-toml-macros`.
- [x] Rewire the main crate to re-export `Toml` from the new macro path.
- [x] Keep legacy donor generator content under [`diver/`](./diver) outside the
  active crate flow; it is not part of the committed macro surface or
  validation path.

## Stage 5: Tests To Crate-Root

- [x] Move physical test files under crate-root `tests/`.
- [x] Keep private implementation suites under [`tests/unit/`](./tests/unit).
- [x] Move reusable emit fixtures under [`tests/support/`](./tests/support).
- [x] Add public integration tests under [`tests/`](./tests).
- [x] End-state requirement met for the active crate surface:
  - [x] no active proof coverage lives in committed `src/*_tests.rs`
  - [x] no active proof coverage lives in committed nested `src/**/test*.rs`

## Stage 6: Golden Fixtures

- [x] Add crate-root golden fixtures for:
  - [x] parse basics
  - [x] emit basics
- [x] Keep committed fixture data under [`tests/golden/`](./tests/golden).
- [x] Move emit test support data out of the source tree.
- [x] Expand crate-root goldens to cover spans, reprojection, and temporal
  formatting edge cases.

## Stage 7: Examples And README Coverage

- [x] Add examples under [`examples/`](./examples) for:
  - [x] parse a TOML document
  - [x] emit a TOML document
  - [x] typed load/store
  - [x] derive macro usage
- [x] Rewrite the README around the minimal public API.
- [x] Document validation commands and benchmark entry points.

## Stage 8: Benchmark Matrix

- [x] Add one clean Criterion bench surface under [`benches/`](./benches).
- [x] Add measured lanes for:
  - [x] parse small document
  - [x] emit small document
  - [x] typed round-trip
- [x] Add crate-local bench script:
  - [x] [`scripts/bench.sh`](./scripts/bench.sh)
- [x] Record the current TOML crate baseline:
  - [x] `rs-toml/parse_small`: about `375-437 ns`
  - [x] `rs-toml/emit_small`: about `633-789 ns`
  - [x] `rs-toml/typed_round_trip`: about `2.30-2.87 µs`
- [x] Keep legacy donor benchmark tooling under [`diver/`](./diver) out of the
  active flow; the top-level Criterion bench and scripts are the only active
  benchmark surface.

## Stage 9: Performance Pass

- [x] Keep the simpler parser/emitter-first crate shape unless benchmarks prove
  a stronger alternative.
- [x] Use measurement as the gate for extra tooling.
- [x] No extra performance helper crate has been adopted in this batch because
  the current active surface already benchmarks and validates cleanly.

## Stage 10: Validation Scripts

- [x] Add:
  - [x] [`scripts/validate.sh`](./scripts/validate.sh)
  - [x] [`scripts/feature-matrix.sh`](./scripts/feature-matrix.sh)
  - [x] [`scripts/bench.sh`](./scripts/bench.sh)
- [x] Run and keep green:
  - [x] `cargo test`
  - [x] `cargo test --doc`
  - [x] `cargo test --examples`
  - [x] `cargo clippy --all-targets --all-features -- -D warnings`
  - [x] `cargo check --benches`

## KDL Interop

- [x] Keep TOML interop additive through [`rs-kdl`](../rs-kdl).
- [x] Follow the same measured pattern as JSON and YAML:
  - [x] direct shared-model TOML bridge
  - [x] generated TOML-on-KDL contract
- [x] Keep TOML temporal values explicit in the neutral model.
- [x] Add proof tests and benchmarks on the KDL side for the TOML adapter.

## Success Criteria

- [x] `rs-toml` is the clear main TOML parser/emitter crate in the workspace.
- [x] Public conversion errors use `thiserror`.
- [x] Tests live under crate-root `tests/`.
- [x] The derive macro lives under [`rs-toml/macros/`](./macros) with a clean
  public story.
- [x] Golden fixtures protect parser and emitter behavior.
- [x] The benchmark matrix is reproducible and crate-local.
- [x] README and examples explain the crate clearly.
- [x] Performance tooling is only adopted when measurement justifies it.
- [x] The public crate story is a simple parser/emitter API, not a donor-style
  feature-fragmented surface.
- [x] The additive KDL interop path follows the same measured shared-model +
  contract rule as the JSON and YAML adapters.
