# Bounded parsing and diagnostic coordinates

Level L3. The approved corrections apply the existing public limits and diagnostic contract across strict, recoverable, typed and detached parsing.

## Plan

- Bound token recognition in the datetime, escape and recovery scanners, preserving inclusive limits and encoded byte offsets.
- Classify unfinished numeric tokens at EOF separately from malformed values with a following delimiter.
- Preserve the most specific Serde key or item span. Resolve native helper coordinates while the source is available.
- Expose the existing path component type and record each missing required item once.
- Exercise the public behavior with integration regressions. Run the repository gate and review the diff before opening a pull request.

The changes affect parser, native and Serde conversion, error types, public exports and integration tests. The risks are token-boundary lookahead, diagnostic accuracy and excessive rescanning when a document has several errors.

## Verification

Five public regression tests failed on v0.4.0 before implementation. Recovery now checks comments and quoted tokens during resynchronization. Datetime and escape tests cover the first excess byte, and EOF tests distinguish truncated values from invalid tokens followed by a newline. Native and Serde tests cover direct extraction, custom key validation, detached item spans, public path component access, duplicate missing-field reports and reverse-order field errors.

The pre-CI gate passed on Windows with one CPU core, one Cargo job and one test thread. Nextest passed 359 tests, 28 executable doctests passed, and 202 applicable API compatibility checks passed against v0.4.0. AgentShield scored 100/100, staged Gitleaks found no secrets, and the harness audit scored 19/39, matching the release baseline.

The all-feature suite passed 402 tests and 31 executable doctests. All-feature Clippy passed with warnings denied. Independent review confirmed that bulk conversion still resolves errors in source order and that both manual and bulk conversion retain their diagnostic order.
