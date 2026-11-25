# 0.4 shared limits and diagnostics

L3. The approved format-family migration adopts rs-format 0.1.0 and Rust 1.100.0.
Native, recoverable and typed parsing must enforce inclusive bounds. Existing
entry points retain their 0.3 limits. Diagnostics retain spans and expose safe
coordinates, including for Serde conversions. Marker tests cover formatters and
sources. The implementation keeps the existing hard nesting and span ceilings.

Work is confined to the library, version metadata, tests and release notes.
The main risks are handling the implicit root, multiline delimiters, composite
numbers and arbitrary visitor errors. Run the workspace gate, all-feature
tests, feature lint and an independent diff review before release.

## Verification

The duplicate-key marker test failed against the unmodified 0.3.1 baseline at
`da8dc7d`. On the migration tree the 395 all-feature tests and 31 executable
doctests passed. The recovery regression checks byte coordinates across
multiple UTF-8 lines with CRLF. The parser maintains a forward position cursor
during recovery and typed conversion sorts diagnostic spans for one source scan.

A one-job Rust 1.100.0 release comparison used the existing 877-byte
`benches/toml_core.rs` small fixture. Ten alternating 1,000-parse samples per
version, after warm-up, yielded median 3,342 ns for 0.3.1 (2,928–3,608 ns)
and 3,569 ns for 0.4.0 (3,181–4,496 ns) on Windows MED. Both ranges overlap.

AgentShield scored 100/100 against the existing baseline. Staged Gitleaks
found zero leaks. The harness audit scored 19/39, matching the prior
repository audit. Host CPU load stopped further local builds after the minimal
Clippy gate flagged a test byte-count expression. The test now counts split
lines. Final gate and review results belong to the pull request.
