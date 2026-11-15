#!/bin/sh
# ECC AgentShield scan (npm:ecc-agentshield, version pinned in .mise.toml).
#
# - Runs the mise-pinned CLI through `mise exec`, so another `agentshield` on
#   PATH (the unrelated Rust crate `agent-shield` also installs one) cannot
#   shadow it. A version mismatch is an error, never a silent pass.
# - Scans git-tracked files plus untracked agent configuration (AGENTS.md,
#   CLAUDE.md, .claude/, .mcp.json, docs/agents/, ...). Other untracked files
#   are skipped: build caches that .gitignore misses can hold hundreds of
#   thousands of files and would fill the temp disk.
# - Compares against agentshield-baseline.json (--gate) when it exists.
#   Refresh it with: ECC_WRITE_BASELINE=1 mise run ecc
# - Exit status is agentshield's own: non-zero fails the gate.
set -eu
cd "$(git rev-parse --show-toplevel)"

want="$(cat .mise.toml mise.toml 2>/dev/null | sed -n 's/^"npm:ecc-agentshield" = "\(.*\)"/\1/p' | head -1)"
[ -n "$want" ] || { echo "ecc: npm:ecc-agentshield is not pinned in mise.toml or .mise.toml" >&2; exit 1; }
got="$(mise exec -- agentshield --version 2>/dev/null || true)"
if [ "$got" != "$want" ]; then
  echo "ecc: agentshield $want required (pinned in .mise.toml), found '${got:-none}'; run: mise install" >&2
  exit 1
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/ecc-scan.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT INT TERM
{
  git ls-files
  git ls-files -o --exclude-standard -- AGENTS.md CLAUDE.md GEMINI.md .mcp.json \
    '.claude/*' '.codex/*' '.cursor/*' '.github/*' '.githooks/*' 'docs/agents/*' 'scripts/*'
} | sort -u | while IFS= read -r f; do
  [ -f "$f" ] && printf '%s\n' "$f"
done | tar -cf - -T - | tar -xf - -C "$tmp"

echo "ecc: agentshield $got scanning $(find "$tmp" -type f | wc -l | tr -d ' ') tracked files"
baseline="$PWD/agentshield-baseline.json"
if [ "${ECC_WRITE_BASELINE:-0}" = 1 ]; then
  # Refresh the committed baseline after reviewing the findings it accepts.
  mise exec -- agentshield baseline write -p "$tmp" -o "$baseline"
elif [ -f "$baseline" ]; then
  # --gate: fail on new critical/high findings or a lower score than the baseline.
  mise exec -- agentshield scan -p "$tmp" --supply-chain --baseline "$baseline" --gate "$@"
else
  mise exec -- agentshield scan -p "$tmp" --supply-chain "$@"
fi
