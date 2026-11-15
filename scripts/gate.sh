#!/usr/bin/env bash
# Pre-CI gate: steps 3-7 of AGENTS.md (fmt, clippy, tests, dependency,
# feature, API and unsafe checks). Stops at the first failure.
# Run with `mise run gate`; the pre-push hook runs it too.
#
# A missing tool is reported as "not installed" and listed again in the
# final summary; it never passes silently.
#
# GATE_BASE=<rev> overrides the comparison base (default: merge-base with
# origin/HEAD, then origin/main). With no base, every conditional check runs.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# ---- Project settings: edit this block per repository --------------------
# API compatibility check for library crates. Set to "off" with a reason for
# application workspaces (all crates `publish = false`).
SEMVER_CHECKS="on"
SEMVER_OFF_REASON=""

# Undefined-behaviour check (miri) when `unsafe` lines change. Set to "off"
# with a reason when the crate forbids unsafe code.
MIRI_CHECKS="on"
MIRI_OFF_REASON=""

# Extra cargo-audit arguments, e.g. documented --ignore RUSTSEC-... entries.
# Keep them identical to the CI audit step.
AUDIT_ARGS=()

# Checks to run before anything else (environment requirements).
preflight() {
  local target="${CARGO_TARGET_DIR:-$PWD/target}"
  case "$PWD" in
    */.worktrees/*) export CARGO_TARGET_DIR="$target/worktrees/${PWD##*/}"; echo "gate: worktree build dir $CARGO_TARGET_DIR" ;;
  esac
}

# Extra test commands that the plain workspace run does not cover,
# for example feature-gated test harnesses.
extra_tests() {
  # CI also builds without default features, and the benches.
  cargo test --workspace --no-default-features --locked
  cargo check --workspace --benches --locked
}
# ---------------------------------------------------------------------------

SKIPPED=""
have() { command -v "$1" >/dev/null 2>&1; }
step() { printf '\n== %s\n' "$*"; }
skip() { echo "$1: not installed"; SKIPPED="$SKIPPED $1"; }

BASE="${GATE_BASE:-}"
if [ -z "$BASE" ]; then
  for ref in origin/HEAD origin/main; do
    if BASE="$(git merge-base HEAD "$ref" 2>/dev/null)"; then break; fi
    BASE=""
  done
fi
if [ -n "$BASE" ]; then
  echo "gate: comparing against $(git rev-parse --short "$BASE")"
else
  echo "gate: no base revision found; running every conditional check"
fi

# True when files matching the regex changed since BASE (committed or not).
changed() {
  [ -z "$BASE" ] && return 0
  git diff --name-only "$BASE" -- | grep -qE "$1"
}

preflight

step "3 fmt"
cargo fmt --all -- --check

step "4 clippy"
cargo clippy --workspace --all-targets -- -D warnings

step "5 tests"
if have cargo-nextest; then
  cargo nextest run --workspace
  # Doctests need a library target; a binary-only workspace has none.
  if cargo metadata --no-deps --format-version 1 \
      | grep -qE '"kind":\["(lib|rlib|dylib|proc-macro)"'; then
    cargo test --workspace --doc
  else
    echo "doctests: n/a (no library target)"
  fi
else
  skip cargo-nextest
  cargo test --workspace
fi
extra_tests

step "6 dependencies"
if changed '(^|/)Cargo\.(toml|lock)$'; then
  if have cargo-deny;    then cargo deny check; else skip cargo-deny;    fi
  if have cargo-audit;   then cargo audit ${AUDIT_ARGS[@]+"${AUDIT_ARGS[@]}"}; else skip cargo-audit;   fi
  if have cargo-machete; then cargo machete;    else skip cargo-machete; fi
else
  echo "no Cargo.toml/Cargo.lock change"
fi

step "6 features"
if [ -z "$BASE" ] || git diff -U0 "$BASE" -- '*Cargo.toml' \
    | grep -qE '^[+-][^+-].*(features|default *=|optional *= *true)'; then
  if have cargo-hack; then
    cargo hack check --workspace --each-feature --no-dev-deps
  else
    skip cargo-hack
  fi
else
  echo "no feature change"
fi

step "6 API"
if [ "$SEMVER_CHECKS" = "off" ]; then
  echo "n/a: $SEMVER_OFF_REASON"
elif [ -z "$BASE" ]; then
  echo "n/a: no base revision to compare against"
elif have cargo-semver-checks; then
  cargo semver-checks --workspace --baseline-rev "$BASE"
else
  skip cargo-semver-checks
fi

step "6 unsafe"
if [ "$MIRI_CHECKS" = "off" ]; then
  echo "n/a: $MIRI_OFF_REASON"
elif [ -n "$BASE" ] && ! git diff -U0 "$BASE" -- '*.rs' | grep -qE '^[+-][^+-].*\bunsafe\b'; then
  echo "no unsafe change"
else
  # Crates owning a changed .rs file: nearest directory with a Cargo.toml.
  crates=""
  for f in $(git diff --name-only "${BASE:-HEAD}" -- '*.rs'); do
    d="$(dirname "$f")"
    while [ "$d" != "." ] && [ ! -f "$d/Cargo.toml" ]; do d="$(dirname "$d")"; done
    case " $crates " in *" $d "*) ;; *) crates="$crates $d" ;; esac
  done
  if [ -z "$crates" ]; then
    echo "no changed Rust files"
  elif cargo +nightly miri --version >/dev/null 2>&1; then
    for d in $crates; do cargo +nightly miri test --manifest-path "$d/Cargo.toml"; done
  else
    echo "affected crates:$crates"
    skip miri
  fi
fi

step "7 result"
if [ -n "$SKIPPED" ]; then
  echo "GATE PASS with checks not run (not installed):$SKIPPED"
else
  echo "GATE PASS"
fi
