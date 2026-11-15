#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo bench -p rs-toml --bench toml_core --manifest-path "$root/Cargo.toml" -- --sample-size 20
