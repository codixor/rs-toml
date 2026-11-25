//! serde errors never echo values from the document (they may be secrets),
//! and integers beyond 64 bits are handled or rejected clearly.
#![cfg(feature = "serde")]

use std::collections::BTreeMap;

use serde::Deserialize;

const SECRET: &str = "hunter2-secret";

fn assert_redacted(err: &rs_toml::serde::Error, secrets: &[&str]) {
    let display = err.to_string();
    let debug = format!("{err:?}");
    for secret in secrets {
        for text in [display.as_str(), debug.as_str(), err.message()] {
            assert!(!text.contains(secret), "{secret:?} leaked in {text:?}");
        }
    }
    assert!(err.span().is_some(), "{err:?}");
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Port {
    port: u16,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
enum Mode {
    Read,
    Write,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct WithMode {
    mode: Mode,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
enum Tagged {
    File { path: String },
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct WithTagged {
    store: Tagged,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Flags {
    enabled: bool,
    ratio: f64,
    name: String,
    c: char,
}

#[test]
fn invalid_type_does_not_echo_string() {
    let e = rs_toml::serde::from_str::<Port>(&format!("port = '{SECRET}'")).unwrap_err();
    assert_eq!(e.message(), "invalid type: string");
    assert_redacted(&e, &[SECRET]);
}

#[test]
fn invalid_value_does_not_echo_integer_or_float() {
    let e = rs_toml::serde::from_str::<Port>("port = 99999999").unwrap_err();
    assert_eq!(e.message(), "invalid value: integer");
    assert_redacted(&e, &["99999999"]);

    let e = rs_toml::serde::from_str::<Port>("port = -12345").unwrap_err();
    assert_redacted(&e, &["12345"]);

    let e = rs_toml::serde::from_str::<Flags>("enabled = 31337.25").unwrap_err();
    assert_redacted(&e, &["31337"]);

    let src = format!("enabled = true\nratio = '{SECRET}'");
    let e = rs_toml::serde::from_str::<Flags>(&src).unwrap_err();
    assert_redacted(&e, &[SECRET]);

    let src = format!("enabled = true\nratio = 1.0\nname = 'n'\nc = '{SECRET}'");
    let e = rs_toml::serde::from_str::<Flags>(&src).unwrap_err();
    assert_redacted(&e, &[SECRET]);
}

#[test]
fn unknown_variant_from_value_is_not_echoed() {
    let e = rs_toml::serde::from_str::<WithMode>("mode = 'sk_live_SECRET'").unwrap_err();
    assert_eq!(e.message(), "unknown variant");
    assert_redacted(&e, &["sk_live_SECRET"]);

    // Externally tagged table form.
    let e = rs_toml::serde::from_str::<WithMode>("mode = { sk_live_SECRET = {} }").unwrap_err();
    assert_redacted(&e, &["sk_live_SECRET"]);

    // Internally tagged enums read the tag through buffered content.
    let e = rs_toml::serde::from_str::<WithTagged>("[store]\ntype = 'sk_live_SECRET'\npath = '/d'")
        .unwrap_err();
    assert_redacted(&e, &["sk_live_SECRET"]);
}

#[test]
fn bool_and_wide_integer_are_not_echoed() {
    let e = rs_toml::serde::from_str::<Port>("port = true").unwrap_err();
    assert_eq!(e.message(), "invalid type: boolean");

    let src = "port = 170141183460469231731687303715884105727";
    let e = rs_toml::serde::from_str::<Port>(src).unwrap_err();
    assert_redacted(&e, &["1701411834604692"]);
}

#[test]
fn unknown_field_hides_the_key_name() {
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct Strict {
        port: u16,
    }
    let e =
        rs_toml::serde::from_str::<Strict>(&format!("port = 1\nprot = '{SECRET}'")).unwrap_err();
    assert!(e.message().starts_with("unknown field"), "{e}");
    assert_redacted(&e, &[SECRET]);
}

#[derive(Debug, PartialEq, Deserialize)]
struct Wide {
    big: i128,
    ubig: u128,
}

#[test]
fn wide_integers_reach_i128_and_u128_fields() {
    let w: Wide = rs_toml::serde::from_str(
        "big = -170141183460469231731687303715884105728\nubig = 170141183460469231731687303715884105727",
    )
    .unwrap();
    assert_eq!(w.big, i128::MIN);
    assert_eq!(w.ubig, i128::MAX as u128);

    let e = rs_toml::serde::from_str::<Wide>("big = 1\nubig = -18446744073709551617").unwrap_err();
    assert_redacted(&e, &["18446744073709551617"]);
}

#[test]
fn wide_integer_in_buffered_content_is_a_clear_error() {
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    #[allow(dead_code)]
    enum U {
        Int(i64),
        Text(String),
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct Outer {
        #[serde(flatten)]
        rest: BTreeMap<String, i64>,
    }
    let src = "a = 1\nb = 18446744073709551616\n";
    let e = rs_toml::serde::from_str::<Outer>(src).unwrap_err();
    assert!(
        e.message()
            .contains("integer out of 64-bit range is not supported here"),
        "{e}"
    );
    assert_eq!(&src[e.span().unwrap()], "18446744073709551616");

    // Untagged enums buffer their input, which cannot hold a value beyond
    // 64 bits; the error says why instead of "did not match any variant".
    let e =
        rs_toml::serde::from_str::<BTreeMap<String, U>>("x = -9223372036854775809").unwrap_err();
    assert!(e.message().contains("64-bit range"), "{e}");

    // In range values still work through buffered content.
    let ok: BTreeMap<String, U> = rs_toml::serde::from_str("x = -5").unwrap();
    assert!(matches!(ok["x"], U::Int(-5)));
}
