//! The derive must not resolve `Some`/`None`/`Ok`/`Err` through the caller's
//! scope: a glob import of same-named items used to break expansion (E0532).
#![cfg(feature = "derive")]
#![allow(dead_code)]

mod shadow {
    pub struct Some;
    pub struct None;
    pub struct Ok;
    pub struct Err;
}

mod user {
    #[allow(unused_imports)]
    use super::shadow::*;
    use rs_toml::Toml;

    #[derive(Debug, Toml)]
    #[toml(Toml)]
    pub struct Config {
        pub name: String,
        pub port: Option<u16>,
    }
}

#[test]
fn derive_ignores_shadowed_prelude_names() {
    let parsed: user::Config = rs_toml::from_str("name = 'app'\nport = 8080\n").unwrap();
    assert_eq!(parsed.name, "app");
    assert_eq!(parsed.port, Option::Some(8080));
    let emitted = rs_toml::to_string(&parsed).unwrap();
    assert!(emitted.contains("port = 8080"));
}
