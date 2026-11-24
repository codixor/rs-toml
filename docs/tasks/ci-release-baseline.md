# Pinned test and API compatibility tools in CI

Level L3. Apply the requested project pins for nextest 0.9.152 and cargo-semver-checks 0.51.0 and run them before publishing any release.

## Plan and acceptance criteria

- Pin the maintainer-provided executables in `mise.toml` for local development and GitHub-hosted Linux CI.
- Use both tools in the reusable CI checks on main and version-tag pushes. Preserve all-feature and minimal-feature tests and doctests, the existing pull request checks, and the release job dependencies.
- Select the public crates.io baseline by SemVer. Main can compare an unchanged manifest version to its published version. A release tag always compares to an earlier version, including on retries. Ignore yanked and future versions. Test these cases and report registry lookup failures.
- Run the pre-push gate and check the main workflow. Version bumps, publication, and modifications to the crates.io environment are outside the change.

## Verification and pre-push gate

Project-level mise resolution selected both pinned versions on Windows. The mise dry run resolved both Linux x64 tools. Actionlint and the baseline-selection regression tests passed. A read-only run against crates.io selected 0.3.1 on main and 0.3.0 for a simulated v0.3.1 tag retry. Both registry-baseline comparisons passed 202 API checks and reported 58 skipped checks. The full native gate passed. Staged Gitleaks found zero leaks, AgentShield scored 100/100 and the independent CI security and production review found no blockers.
