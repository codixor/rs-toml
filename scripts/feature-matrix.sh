#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cargo test -p rs-toml --manifest-path "$root/Cargo.toml"
cargo test -p rs-toml --no-default-features --manifest-path "$root/Cargo.toml"
cargo test -p rs-toml --features derive --manifest-path "$root/Cargo.toml"
cargo test -p rs-toml --features serde --manifest-path "$root/Cargo.toml"
cargo test -p rs-toml --all-features --manifest-path "$root/Cargo.toml"
