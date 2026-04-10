# rs-toml-macros

Derive macros for [rs-toml](https://crates.io/crates/rs-toml).

[![Crates.io](https://img.shields.io/crates/v/rs-toml-macros?style=flat-square)](https://crates.io/crates/rs-toml-macros)
[![Docs.rs](https://img.shields.io/docsrs/rs-toml-macros?style=flat-square)](https://docs.rs/rs-toml-macros/latest/rs_toml_macros/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue?style=flat-square)](LICENSE-MIT)

This crate provides `#[derive(Toml)]` for automatically generating `FromToml` and `ToToml` implementations. It is designed to be used with [rs-toml](https://crates.io/crates/rs-toml).

The easiest way to use this crate is by enabling the `derive` feature on `rs-toml`, which re-exports everything automatically:

```toml
[dependencies]
rs-toml = { version = "0.1", features = ["derive"] }
```

You can also depend on `rs-toml-macros` directly alongside `rs-toml` as a compile time reduction technique, since it allows Cargo to begin compiling the macros in parallel with `rs-toml` itself. That said, `rs-toml-macros` is dependency free and compiles quickly, so in most non-trivial projects the difference is negligible and using the `derive` feature is equally fast.

For documentation on the derive macro and its attributes, see the [rs-toml docs](https://docs.rs/rs-toml/latest/rs_toml/derive.Toml.html).

## License

This contribution is dual licensed under EITHER OF

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
