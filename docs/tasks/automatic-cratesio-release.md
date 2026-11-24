# Automated crates.io release

Level: L3. Apply the release process already approved for rs-yaml to the two published members of the rs-toml workspace.

## Plan

- Bump the workspace and its derive dependency to 0.3.1. Refresh the local workspace entries in the lockfiles.
- Run CI and TOML conformance on pushes to main or a matching version tag before publishing. Keep the existing pull request and scheduled checks.
- Use the existing cratesio environment. Publish rs-toml-macros before rs-toml and tag the verified source commit. Recheck the registry inside the publish job so retries can finish a partially published workspace.
- Validate matching and mismatched tags, repeated releases and partial recovery. Run the repository gate and review the workflow before pushing. Verify the public registry archives and matching GitHub release afterward.

The change is limited to release metadata, automation and documentation. A partial registry publication is permanent, so both members must have consistent source provenance and recovery must skip a member already published.

## Pre-push verification

- Windows checkout initially converted fixture newlines and nine emitter tests failed. The test-data attribute preserves the committed bytes for all 15 TOML fixtures. The workspace tests and doctests passed with those bytes, including 325 parser library tests.
- Formatting, Clippy, bench compilation, and cargo-deny passed. Cargo packaged both workspace members with the lockfile. The local gate reported five unavailable tools: cargo-nextest, cargo-audit, cargo-machete, cargo-hack and cargo-semver-checks. Cargo ran the tests directly.
- AgentShield 1.6.0 scored 100/100 for the agent configuration. Gitleaks found zero staged leaks. An independent review checked the workflow permissions and release recovery, with 24 offline cases for planning, archive provenance and tag reservation.
- A partial publication can resume only at the tested commit. To recover, rerun the original workflow or push its matching tag. When both packages already exist, the workflow can repair a missing GitHub release at their verified commit.
