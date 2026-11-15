#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo test -p rs-toml --manifest-path "$root/Cargo.toml"
cargo test -p rs-toml --doc --manifest-path "$root/Cargo.toml"
cargo test -p rs-toml --examples --manifest-path "$root/Cargo.toml"
cargo clippy -p rs-toml --all-targets --all-features --manifest-path "$root/Cargo.toml" -- -D warnings
cargo check -p rs-toml --benches --manifest-path "$root/Cargo.toml"
