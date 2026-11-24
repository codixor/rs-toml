# Tag-only releases

Level L3. Apply the approved rs-yaml tag-only release policy. The release workflow must run CI and TOML conformance on main and publish both workspace packages only for a pushed tag matching the workspace version.

## Acceptance criteria

- A main push runs checks and skips release planning and publishing.
- A matching version tag points at the tested commit before the macros and parser can publish, in that order. The GitHub release uses that tag.
- Partial and complete releases can be retried. A mismatched tag or registry commit stops publication.

## Plan

Update the conditions in `.github/workflows/release.yml`, the checks in `scripts/release.py` and the release instructions in `README.md`. Exercise main, invalid tags and retries with isolated Git fixtures. Run the pre-push gate and inspect the jobs for the main push. Crate versions and macros are outside this change.

## Verification

An isolated Git fixture reproduced the previous planner accepting main. After the change the planner rejects main, a branch named like a version, a mismatched tag, an absent tag, a tag at another commit and conflicting crate provenance. Lightweight tags, annotated tags and retries after a partial publication passed. Actionlint passed on the release workflow.

## Pre-CI gate

The remote-tag check accepted lightweight and annotated tags and stopped on a moved or deleted tag. A read-only check against v0.3.1 found both published crate archives at the tagged commit and the existing GitHub release.

Formatting, Clippy, workspace tests with default and minimal features, doctests and benchmark compilation passed. Cargo ran the suites directly because nextest was unavailable. The local gate also reported cargo-semver-checks unavailable. The change touches release automation and documentation. Staged Gitleaks found no secrets, AgentShield scored 100/100, and independent review found no open security or production issues.
