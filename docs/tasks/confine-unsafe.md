# confine-unsafe: remove and confine `unsafe` for 0.3.0
Level: L3 (`unsafe`, layout, parser fast paths, performance)
Goal: keep rs-toml's performance while removing as much `unsafe` as possible and
confining the rest to a small allow-listed set of modules, with
`#![deny(unsafe_code)]` everywhere else.
Acceptance criteria:
- [ ] Extended criterion bench (`benches/toml_core.rs`): small/medium/large parse,
      emit/format, serde deserialize, `FromToml` derive; baseline `base` recorded here
- [ ] Every remaining `unsafe` lives in the allow-listed module(s); crate root has
      `#![deny(unsafe_code)]`; `scripts/check-unsafe.sh` fails on `unsafe` elsewhere
      and runs in CI
- [ ] Each kept change regresses no bench by more than 2% (or the reason it was
      reverted is recorded below)
- [ ] Public API and behaviour unchanged; golden output byte-identical; nesting
      limit and error redaction unchanged
- [ ] Final gate (fmt, clippy all-features, tests all/no-default features, Miri
      strict provenance, toml-test, 4 fuzz targets x 60 s) passes
- [ ] Version 0.3.0 everywhere (workspace, macros dep, lockfiles incl. fuzz/ and
      diver/toml-test-harness) and CHANGELOG entry
Out of scope: public type layout changes, new dependencies, pushing/publishing.
Likely files: src/{arena,item,item/table,item/array,parser,error,time,de,helper,emit,
emit/partition,emit/normalization,lib}.rs, macros/src/util.rs, benches/toml_core.rs,
scripts/check-unsafe.sh, .github/workflows/ci.yml, Cargo.toml, CHANGELOG.md.
Risks: performance regressions in the parser hot loop; soundness of the confined
API (it must be impossible to cause UB from safe crate code); Miri coverage.

## Plan

Resource limits for this workstation (coordinator): one heavy cargo job at a time,
`CARGO_BUILD_JOBS=4`, criterion `--sample-size 20 --measurement-time 3`,
fuzz `-rss_limit_mb=1024 -jobs=1`, generated bench inputs <= 10 MB built once,
commit after each successful step.

1. Bench + baseline (commit).
2. Classify every block (table below).
3. Safe rewrites, one group per commit, each followed by tests + bench vs `base`:
   - (d) misc: helper.rs `ptr::read` shallow copy, de.rs bitset/offset_from,
     time.rs `Raw` cast + MaybeUninit buffer, error.rs `from_utf8_unchecked`,
     emit.rs prefix allocations, emit/partition.rs, emit/normalization.rs casts.
   - (a) parser fast paths: `str_slice`, `from_utf8_unchecked` over committed
     scratch, key pointer.
   - (b) MaybeUninit / raw-write tricks.
4. (c) Confine the arena, the scratch buffer, the `Item` payload union, the
   `Table`/`Array`/`MaybeItem` reinterpretation casts and `MaybeTomlPath` into
   allow-listed modules exposing safe, tag-checked accessors. Unchecked
   accessors used by the parser become checked (`match`/`expect`-free
   `Option`) unless the bench shows > 2% cost.
5. Enforcement: `#![deny(unsafe_code)]` in lib.rs, `#[allow(unsafe_code)]` on the
   confined modules only, `scripts/check-unsafe.sh` in CI; macros crate safe.
6. Version bump, CHANGELOG, final gate.

## Classification
(filled in during step 2)

## Baseline
(filled in during step 1)

## Progress

## Decisions
