# Pre-CI gate

Nothing is pushed, and no pull request is opened, until the full gate passes
locally. CI confirms a green tree; it is not where problems are found.

The gate uses the ECC (Everything Claude Code) plugin where the harness has
it. If a step's command is not available, say so in the report and run the
local fallback; never skip a step silently.

## Steps, in order (stop at the first failure)

| # | Step | Run by | ECC | Local fallback |
|---|---|---|---|---|
| 1 | Harness and repo audit: hooks, skills, agents, MCP config, secrets surface | Flash | `/harness-audit repo` | — (report "not available") |
| 2 | Secret and config scan | Flash | AgentShield scan | review `git diff --cached` for keys, tokens, `.env`; `gitleaks protect --staged` |
| 3 | Format | Flash | — | `cargo fmt --all -- --check` |
| 4 | Lint | Flash | — | `cargo clippy --workspace --all-targets -- -D warnings` |
| 5 | Tests | Flash | — | `cargo nextest run --workspace` + `cargo test --workspace --doc` when nextest is installed, else `cargo test --workspace`; then `extra_tests` from `scripts/gate.sh` |
| 6 | Dependencies, features, API, unsafe | Flash | — | see below |
| 7 | Quality gate | Flash | `/quality-gate` | steps 3–6 all green |
| 8 | Security review (L2–L3) | Pro (thinking) | `security-review` skill | Pro reviews the diff for auth, input handling, `unsafe`, secrets, error leaks |
| 9 | Production audit (L3, or config, CI, migrations, auth, deployment) | Pro (thinking) | `production-audit` skill | Pro checks config, migrations and rollback against the diff |

Steps 1–7 are deterministic and belong to Flash; 8–9 need judgement and
belong to Pro. Any finding goes back to the coder; the gate restarts from
step 1 after the fix.

### Step 6 detail

`scripts/gate.sh` compares the tree with the merge base of `origin/HEAD`
(override with `GATE_BASE=<rev>`). Without a base, every conditional check runs.

| Trigger | Check |
|---|---|
| `Cargo.toml` or `Cargo.lock` changed | `cargo deny check`, `cargo audit`, `cargo machete` |
| a `[features]`, `default` or `optional` line changed | `cargo hack check --workspace --each-feature --no-dev-deps` |
| always (library crates) | `cargo semver-checks --workspace --baseline-rev <base>` — set `SEMVER_CHECKS="off"` with a reason for application workspaces |
| a line containing `unsafe` added or removed | `cargo +nightly miri test` on each owning crate |

A tool that is not installed prints `<tool>: not installed`, and the run
ends with `GATE PASS with checks not run (not installed): …` instead of
`GATE PASS`. Report that result as it is; install the tool
([tools.md](tools.md)) or name the missing check in the gate report.

## One command and the hook

- `mise run gate` runs steps 3–7 (`scripts/gate.sh`).
- `.githooks/pre-push` runs `mise run gate`. Enable it once per clone:
  `git config core.hooksPath .githooks`.
- The hook covers the mechanical checks only. Steps 1–2 (ECC audit, secret
  scan) and 8–9 (reviews) are still run by the agents before pushing.
- `git push --no-verify` bypasses the hook; agents must never use it.

## Report

```
PRE-CI GATE  <commit sha>
1 harness-audit   pass | fail | n/a (reason)
2 secret scan     pass | fail
3 fmt             pass | fail
4 clippy          pass | fail
5 tests           pass | fail
6 deps/api        pass | fail | partial (tools not installed: …)
7 quality         pass | fail
8 security        pass | fail | skipped (L0–L1)
9 production      pass | fail | skipped (L0–L2, no config/CI/auth change)
RESULT            READY TO PUSH | BLOCKED (step N)
```

Only `READY TO PUSH` allows a push.

## Protecting CI from repeated failures

- **One push per verified state.** Never push to "see if CI passes".
- **CI failure stops the loop.** Flash reads the CI log, reproduces the
  failure locally and hands it to Pro. No new push until the fix passes the
  whole gate again.
- **Two strikes.** After two CI failures on the same branch, stop and report
  to the human with both logs. Do not push a third time.
- **Never weaken a gate to pass.** No `#[allow]`, `#[ignore]`, skipped tests,
  lowered lint levels or `continue-on-error` unless the human approves that
  exact change.
- **Flaky tests** are reported, not retried in a loop: rerun once locally; if
  it passes and fails at random, record it and stop.
- **CI config itself** (workflow files, `deny.toml`, audit allow-lists)
  is L3: plan first, production audit (step 9) before push.

Recommended CI settings to match:

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true          # a new push cancels the old run

jobs:
  check:
    steps:
      - run: cargo fmt --all -- --check          # cheapest first
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
      # fail fast: no continue-on-error
```
