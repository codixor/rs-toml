# Tools

Install once per machine. The gate uses a tool when present and prints
`not installed` otherwise (it never passes silently).

| Tool | Use | Gate step |
|---|---|---|
| `cargo-nextest` | faster test runs; doctests still need `cargo test --doc` | 5 |
| `cargo-deny`, `cargo-audit` | licences, bans, advisories | 6 |
| `cargo-machete` | unused dependencies | 6 |
| `cargo-hack` | feature combinations | 6 |
| `cargo-semver-checks` | public API compatibility | 6 |
| `miri` (nightly) | undefined behaviour in `unsafe` | 6 |
| `gitleaks` | staged secret scan | 2 |
| `cargo-llvm-cov` | coverage on changed code | L2–L3 review |
| `proptest`, `insta` | property and snapshot tests | in tests |
| `bacon` / `cargo-watch` | re-check on save | coder |
| `sccache` | shared build cache | local, CI |

Install the project-pinned nextest and cargo-semver-checks with `mise install`. To install additional gate tools:

```sh
cargo install --locked cargo-machete cargo-hack
rustup toolchain install nightly --component miri
```
