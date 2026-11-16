//! `FromToml` errors must not quote document values: configs carry secrets.
#![cfg(feature = "derive")]

use rs_toml::Toml;

#[derive(Debug, Toml)]
#[toml(Toml)]
enum Mode {
    Fast,
    Slow,
}

#[derive(Debug, Toml)]
#[toml(Toml)]
struct Config {
    mode: Mode,
}

#[test]
fn unknown_variant_error_does_not_quote_the_value() {
    let source = "mode = 'sk_live_SECRET'\n";
    let err = rs_toml::from_str::<Config>(source).unwrap_err();
    let shown = format!("{err} {err:?}");
    assert!(!shown.contains("sk_live_SECRET"), "{shown}");
    let rendered: Vec<String> = err.errors.iter().map(|e| e.message(source)).collect();
    for message in &rendered {
        assert!(!message.contains("sk_live_SECRET"), "{message}");
    }
    assert!(
        rendered.iter().any(|m| m.contains("unknown variant")),
        "{rendered:?}"
    );
}
