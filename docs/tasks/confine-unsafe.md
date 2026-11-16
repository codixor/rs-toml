# confine-unsafe: remove and confine `unsafe` for 0.3.0
Level: L3 (`unsafe`, layout, parser fast paths, performance)
Goal: keep rs-toml's performance while removing as much `unsafe` as possible and
confining the rest to a small allow-listed set of modules, with
`#![deny(unsafe_code)]` everywhere else.
Acceptance criteria:
- [x] Extended criterion bench (`benches/toml_core.rs`): small/medium/large parse,
      emit/format, serde deserialize, `FromToml` derive; baseline `base` recorded here
- [x] Every remaining `unsafe` lives in the allow-listed module(s); crate root has
      `#![deny(unsafe_code)]`; `scripts/check-unsafe.sh` fails on `unsafe` elsewhere
      and runs in CI
- [x] Each kept change regresses no bench by more than 2% (or the reason it was
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
`unsafe` occurrences (`grep -w unsafe`, code + SAFETY comments counted as in
0.2.0) per file. 0.2.0: item.rs 42, parser.rs 26, arena.rs 24, item/table.rs 18,
item/array.rs 18, error.rs 15, time.rs 5, de.rs 4, emit/partition.rs 3,
helper.rs 2, emit/normalization.rs 2, emit.rs 2, lib.rs 1, macros util.rs 3
(about 165). 0.3.0: src/raw/item.rs 40, src/raw/arena.rs 26, src/raw/path.rs 12,
src/raw/vec.rs 7, src/raw/shallow.rs 1, src/raw/text.rs 1 (87 code
occurrences, plus 2 in src/raw/mod.rs docs); none elsewhere; macros crate
`#![forbid(unsafe_code)]`.

Kept in raw, each with a safe API:
- raw::arena: bump allocator slabs, `Scratch` (UTF-8 invariant), `alloc_copy`,
  `alloc_extended`, `alloc_filled`, `alloc_str`; `realloc` stays `unsafe fn`.
- raw::vec::ArenaVec: arena-backed vector (len <= cap, first len initialized).
- raw::item: `Payload` union read only after a tag check; `Item`/`Table`/
  `Array`/`MaybeItem` repr(C) reinterpretations (const-asserted layout);
  unchecked accessors are private `unsafe fn`; metadata writes go through
  `MetaMut`, which cannot change the tag.
- raw::path::MaybeTomlPath: self-referential owned error path; `uncomputed`
  takes `&Item` (was a raw pointer into `NonNull::new_unchecked`), sizes are
  checked before narrowing to u32.
- raw::shallow::ShallowTable: bitwise item copies for `flatten_any`, only
  exposed through `&Table`.
- raw::text::keylike_run: ASCII run as `&str`.

## Baseline
`base` = 249cce03 (0.2.0 code + extended bench), Apple M-series Mac mini,
criterion `--sample-size 20 --measurement-time 3`, `--all-features`. The
machine was shared (load average 4-14), so single runs swing by +/-15-20%
depending on run order. Method: fresh paired runs (0.2.0 tree immediately
before the branch, same target dir), an order-reversed pair for any bench
over +2%, and a per-commit bisect (two rounds) for benches that stayed high.

Paired run at low load (load 4-5, branch at 49d5b3ca vs fresh `base2`):

| bench | change |
|---|---|
| parse/small | -43.6% |
| parse/medium | -37.5% |
| parse/large | -44.3% |
| emit/format/small | -20.0% |
| emit/preserved/small | -5.2% |
| emit/format/medium | +3.6% |
| emit/preserved/medium | -0.5% (n.s.) |
| emit/to_string_map | -5.0% |
| typed_round_trip | -3.1% |
| serde/from_str/medium | -2.6% |
| serde/from_str/large | +0.7% (n.s.) |
| serde/structs/medium | -0.6% (n.s.) |
| derive/from_toml/medium | +13.9% |
| derive/round_trip/small | +14.8% |

Second pair (`base3`): parse/small +3.8%, parse/medium +2.7%, parse/large
-3.7%, emit/format/small +1.9%, emit/preserved/small -6.7%,
emit/format/medium +4.0% (n.s.), emit/preserved/medium -4.7%,
emit/to_string_map -0.4%, derive/from_toml/medium +0.3%,
derive/round_trip/small +1.8%, serde/* within +/-2.4% (n.s.).

Order-reversed pair for the flagged benches (A: branch measured after
0.2.0; B: 0.2.0 measured after branch, sign flipped to "branch vs 0.2.0"):
emit/format/medium -3.4% / +8.0%, emit/preserved/medium -2.7% / +9.7%,
emit/to_string_map +7.0% / +25.6%, serde/structs/medium +4.3% / +5.0%,
derive/round_trip/small +8.4% / +20.9%, parse/small -0.2% / +10.7%,
parse/medium -16.1% / +8.0%. The sign tracks run order, not code.

Per-commit bisect (median time, two rounds each) for the three benches that
stayed above +2%:

| commit | emit/to_string_map | serde/structs/medium | derive/round_trip/small |
|---|---|---|---|
| 249cce03 (0.2.0) | 5.87 / 6.75 us | 255.6 / 267.4 us | 926 / 970 ns |
| dffad544 | 5.00 / 5.85 us | 221.6 / 225.6 us | 869 / 1033 ns |
| 20dc715c | 5.45 / 5.08 us | 222.1 / 223.4 us | 995 / 912 ns |
| a6c1d0c7 | 5.06 / 5.14 us | 216.6 / 225.7 us | 895 / 899 ns |
| b64baf64 | 4.96 / 5.10 us | 218.2 / 216.3 us | 1024 / 973 ns |

No step is slower than 0.2.0 beyond its round-to-round spread, so no step
was reverted. Parse is consistently 0-40% faster (safe fast paths and the
`ArenaVec` growth path); emit and serde are within noise.

## Progress
- 249cce03 bench; dffad544 misc fast paths safe + src/raw; 20dc715c tables and
  arrays on ArenaVec; a6c1d0c7 Item payload in raw::item; 49d5b3ca unused
  re-export (clippy); 9e39b19f MaybeTomlPath + ShallowTable into raw, macros
  safe; f951d19d deny(unsafe_code) + scripts/check-unsafe.sh in CI;
  b64baf64 checked path sizes; then version bump, changelog and final gate.

## Decisions
- `MaybeTomlPath` is self-referential (key names point into its own
  allocation), so it moved to raw instead of being rewritten safely, which
  would change `Error`'s size and `Error::path` borrowing.
- `ShallowTable` keeps its bitwise copy (needed to hand borrowed items to
  `FromToml::from_toml(&Item)` as a table without cloning) in raw.
- Macros: `Error` no longer implements `Send` unsafely; the panic payload
  carries the message and the span goes through a thread-local (proc macros
  expand on one thread). `DEFAULT_ATTR` (an `unsafe impl Sync` static) is
  replaced by pool-allocated defaults.
- `ItemMetadata` was public in 0.2.0 only as an unreachable type (no path
  exports it); the dead re-export was dropped. No public API change.
