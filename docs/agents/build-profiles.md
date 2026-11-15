# Build profiles

Fast iteration in development, maximum speed in release. Keep these in the
workspace `Cargo.toml`; do not change them per task. Shared standard:
`/Volumes/Home/DevMemory/RUST-BUILD.md` (size profile, rustflags, UPX policy).

```toml
[profile.dev]
opt-level = 0
debug = "line-tables-only"   # file:line in backtraces, far less debug info
incremental = true

[profile.dev.package."*"]
opt-level = 2                # dependencies compiled once, then run fast
debug = false

[profile.dev.build-override]
opt-level = 3                # build scripts and proc macros run faster

[profile.test]
inherits = "dev"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
incremental = false
debug = false
strip = "symbols"
panic = "unwind"             # keep unwind unless the project decides on "abort"

[profile.profiling]          # cargo build --profile profiling
inherits = "release"
debug = "line-tables-only"
strip = "none"
```

Optional, machine-local `.cargo/config.toml` (not committed when machine-specific):

```toml
[target.x86_64-unknown-linux-gnu]   # Linux: mold if installed
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]

[profile.dev]                       # macOS: keep split debug info
split-debuginfo = "unpacked"
```

Notes:
- `panic = "abort"` makes binaries smaller but stops `catch_unwind` and unwinding cleanup; project decision only.
- Never set `-C target-cpu=native` for binaries distributed to other machines.
- `lto = "thin"` if release builds become too slow for CI; the runtime difference is usually small.
