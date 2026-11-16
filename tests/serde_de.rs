//! serde `Deserialize` support (`rs_toml::serde`), with shapes modelled on a
//! real service configuration and a differential check against the `toml`
//! crate.
#![cfg(feature = "serde")]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Deserializer};

fn yes() -> bool {
    true
}

fn default_mode() -> u32 {
    0o660
}

fn default_timeout() -> Duration {
    Duration::from_secs(10)
}

fn unix_mode<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    match String::deserialize(d)?.as_str() {
        "0660" => Ok(0o660),
        "0600" => Ok(0o600),
        _ => Err(serde::de::Error::custom("use \"0660\" or \"0600\"")),
    }
}

fn duration<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
    let s = String::deserialize(d)?;
    let secs = s
        .strip_suffix('s')
        .unwrap_or(&s)
        .parse::<u64>()
        .map_err(serde::de::Error::custom)?;
    Ok(Duration::from_secs(secs))
}

fn some_uses<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Use>, D::Error> {
    let uses = Vec::<Use>::deserialize(d)?;
    if uses.is_empty() {
        return Err(serde::de::Error::custom("use must name at least one use"));
    }
    Ok(uses)
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Config {
    listen: String,
    #[serde(default)]
    storage: Option<Storage>,
    #[serde(default)]
    audit: Vec<Audit>,
    #[serde(default)]
    operators: Vec<String>,
    #[serde(default = "yes")]
    mlock: bool,
    #[serde(default)]
    encrypted_volume_uuid: Option<String>,
    #[serde(default)]
    outbound: Vec<Outbound>,
    #[serde(default)]
    unix: Option<Unix>,
    #[serde(default)]
    limits: Limits,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Unix {
    path: PathBuf,
    #[serde(default = "default_mode", deserialize_with = "unix_mode")]
    mode: u32,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum Storage {
    File { path: PathBuf },
    Postgres(Postgres),
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Postgres {
    connection: String,
    #[serde(default)]
    password_file: Option<PathBuf>,
    #[serde(default = "default_timeout", deserialize_with = "duration")]
    timeout: Duration,
    #[serde(default = "yes")]
    create_table: bool,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum Audit {
    File { path: PathBuf },
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Outbound {
    url: String,
    #[serde(rename = "use", default, deserialize_with = "some_uses")]
    uses: Vec<Use>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Use {
    Jwt,
    Webhooks,
    Audit,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
struct Limits {
    max_connections: u16,
    burst: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_connections: 64,
            burst: 100,
        }
    }
}

const FULL: &str = r#"
listen = "127.0.0.1:8200"
operators = ["spiffe://example.org/ops", "sha256:abcd"]
mlock = false

[storage]
type = "postgres"
connection = "host=db dbname=vault"
timeout = "30s"

[[audit]]
type = "file"
path = "/var/log/audit.jsonl"

[[outbound]]
url = "https://idp.example"
use = ["jwt", "webhooks"]

[[outbound]]
url = "https://siem.example"

[unix]
path = "/run/vault.sock"
mode = "0600"

[limits]
burst = 7
"#;

fn expected_full() -> Config {
    Config {
        listen: "127.0.0.1:8200".into(),
        storage: Some(Storage::Postgres(Postgres {
            connection: "host=db dbname=vault".into(),
            password_file: None,
            timeout: Duration::from_secs(30),
            create_table: true,
        })),
        audit: vec![Audit::File {
            path: "/var/log/audit.jsonl".into(),
        }],
        operators: vec!["spiffe://example.org/ops".into(), "sha256:abcd".into()],
        mlock: false,
        encrypted_volume_uuid: None,
        outbound: vec![
            Outbound {
                url: "https://idp.example".into(),
                uses: vec![Use::Jwt, Use::Webhooks],
            },
            Outbound {
                url: "https://siem.example".into(),
                uses: vec![],
            },
        ],
        unix: Some(Unix {
            path: "/run/vault.sock".into(),
            mode: 0o600,
        }),
        limits: Limits {
            max_connections: 64,
            burst: 7,
        },
    }
}

fn err<T: serde::de::DeserializeOwned + Debug>(src: &str) -> rs_toml::serde::Error {
    rs_toml::serde::from_str::<T>(src).unwrap_err()
}

/// The source text an error points at.
fn spanned<'a>(src: &'a str, e: &rs_toml::serde::Error) -> &'a str {
    &src[e.span().expect("error has a span")]
}

#[test]
fn full_config() {
    let cfg: Config = rs_toml::serde::from_str(FULL).unwrap();
    assert_eq!(cfg, expected_full());
}

#[test]
fn defaults_and_missing_options() {
    let cfg: Config = rs_toml::serde::from_str("listen = 'x'").unwrap();
    assert!(cfg.mlock);
    assert_eq!(cfg.storage, None);
    assert_eq!(cfg.audit, []);
    assert_eq!(cfg.limits, Limits::default());
    assert_eq!(cfg.unix, None);
}

#[test]
fn deserialize_with_default_is_used_when_missing() {
    let cfg: Config = rs_toml::serde::from_str("listen = 'x'\n[unix]\npath = '/s'").unwrap();
    assert_eq!(cfg.unix.unwrap().mode, 0o660);
}

#[test]
fn deserialize_with_custom_error_points_at_value() {
    let src = "listen = 'x'\n[unix]\npath = '/s'\nmode = '0777'\n";
    let e = err::<Config>(src);
    assert!(e.message().contains("use \"0660\""), "{e}");
    assert_eq!(spanned(src, &e), "'0777'");
}

#[test]
fn deserialize_with_rejects_empty_vec() {
    let src = "listen = 'x'\n[[outbound]]\nurl = 'u'\nuse = []\n";
    let e = err::<Config>(src);
    assert_eq!(e.message(), "use must name at least one use");
    assert_eq!(spanned(src, &e), "[]");
}

#[test]
fn internally_tagged_file_variant() {
    let cfg: Config =
        rs_toml::serde::from_str("listen = 'x'\n[storage]\ntype = 'file'\npath = '/d'").unwrap();
    assert_eq!(cfg.storage, Some(Storage::File { path: "/d".into() }));
}

#[test]
fn internally_tagged_unknown_variant_points_at_table() {
    let src = "listen = 'x'\n[storage]\ntype = 's3'\npath = '/d'\n";
    let e = err::<Config>(src);
    assert!(e.message().starts_with("unknown variant, expected"), "{e}");
    assert!(!e.message().contains("s3"), "{e}");
    assert_eq!(spanned(src, &e), "'s3'");
}

#[test]
fn internally_tagged_deny_unknown_fields() {
    let src = "listen = 'x'\n[storage]\ntype = 'file'\npath = '/d'\nextra = 1\n";
    let e = err::<Config>(src);
    assert!(e.message().starts_with("unknown field `extra`"), "{e}");
    assert!(e.span().is_some());
}

#[test]
fn unknown_field_span_is_the_key() {
    let src = "listen = 'x'\n[limits]\nburst = 1\nburts = 2\n";
    let e = err::<Config>(src);
    assert!(e.message().starts_with("unknown field `burts`"), "{e}");
    assert_eq!(spanned(src, &e), "burts");
}

#[test]
fn unknown_top_level_field_span_is_the_key() {
    let src = "listen = 'x'\nlissten = 'y'\n";
    let e = err::<Config>(src);
    assert_eq!(spanned(src, &e), "lissten");
}

#[test]
fn wrong_type_span_is_the_value() {
    let src = "listen = 'x'\nmlock = 'yes'\n";
    let e = err::<Config>(src);
    assert!(e.message().starts_with("invalid type: string"), "{e}");
    assert_eq!(spanned(src, &e), "'yes'");
}

#[test]
fn out_of_range_integer_span_is_the_value() {
    let src = "listen = 'x'\n[limits]\nmax_connections = 70000\n";
    let e = err::<Config>(src);
    assert!(e.message().contains("expected u16"), "{e}");
    assert_eq!(spanned(src, &e), "70000");
}

#[test]
fn missing_field_span_is_the_enclosing_table() {
    let src = "listen = 'x'\n[unix]\nmode = '0600'\n";
    let e = err::<Config>(src);
    assert_eq!(e.message(), "missing field `path`");
    let s = spanned(src, &e);
    assert!(s.contains("mode = '0600'"), "{s:?}");
    assert!(!s.contains("listen"), "{s:?}");
}

#[test]
fn missing_top_level_field() {
    let e = err::<Config>("mlock = true");
    assert_eq!(e.message(), "missing field `listen`");
    assert_eq!(e.span(), Some(0..12));
}

#[test]
fn error_inside_array_of_tables_points_at_element_value() {
    let src = "listen = 'x'\n[[outbound]]\nurl = 'a'\n[[outbound]]\nurl = 2\n";
    let e = err::<Config>(src);
    assert_eq!(spanned(src, &e), "2");
}

#[test]
fn syntax_error_has_span() {
    let src = "listen = \n";
    let e = err::<Config>(src);
    assert!(e.span().is_some(), "{e:?}");
}

#[test]
fn unknown_unit_variant_points_at_value() {
    let src = "listen = 'x'\n[[outbound]]\nurl = 'a'\nuse = ['jwt', 'smtp']\n";
    let e = err::<Config>(src);
    assert!(e.message().starts_with("unknown variant, expected"), "{e}");
    assert!(!e.message().contains("smtp"), "{e}");
    assert_eq!(spanned(src, &e), "'smtp'");
}

#[test]
fn integer_ranges_never_wrap() {
    #[derive(Debug, Deserialize)]
    struct I {
        #[serde(default)]
        u8: Option<u8>,
        #[serde(default)]
        u16: Option<u16>,
        #[serde(default)]
        u32: Option<u32>,
        #[serde(default)]
        u64: Option<u64>,
        #[serde(default)]
        i8: Option<i8>,
        #[serde(default)]
        i16: Option<i16>,
        #[serde(default)]
        i32: Option<i32>,
        #[serde(default)]
        i64: Option<i64>,
        #[serde(default)]
        usize: Option<usize>,
    }
    let ok: I = rs_toml::serde::from_str(
        "u8 = 255\nu16 = 65535\nu32 = 4294967295\nu64 = 9223372036854775807\n\
         i8 = -128\ni16 = -32768\ni32 = -2147483648\ni64 = -9223372036854775808\nusize = 0",
    )
    .unwrap();
    assert_eq!(
        (ok.u8, ok.u16, ok.u32, ok.u64, ok.usize),
        (
            Some(255),
            Some(65535),
            Some(u32::MAX),
            Some(i64::MAX as u64),
            Some(0)
        )
    );
    assert_eq!(
        (ok.i8, ok.i16, ok.i32, ok.i64),
        (
            Some(i8::MIN),
            Some(i16::MIN),
            Some(i32::MIN),
            Some(i64::MIN)
        )
    );
    for bad in [
        "u8 = 256",
        "u8 = -1",
        "u16 = 65536",
        "u32 = 4294967296",
        "u64 = -1",
        "i8 = 128",
        "i8 = -129",
        "i16 = 32768",
        "i32 = 2147483648",
        "usize = -1",
    ] {
        let e = err::<I>(bad);
        let value = bad.split(" = ").nth(1).unwrap();
        assert_eq!(spanned(bad, &e), value, "{bad}: {e}");
    }
}

#[test]
fn floats_and_integers_into_floats() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct F {
        a: f64,
        b: f32,
        c: f64,
        d: f64,
    }
    let f: F = rs_toml::serde::from_str("a = 1.5\nb = 2.25\nc = 3\nd = inf").unwrap();
    assert_eq!(
        f,
        F {
            a: 1.5,
            b: 2.25,
            c: 3.0,
            d: f64::INFINITY
        }
    );
    let n: BTreeMap<String, f64> = rs_toml::serde::from_str("x = nan").unwrap();
    assert!(n["x"].is_nan());
}

#[test]
fn borrowed_strings_from_document() {
    #[derive(Deserialize)]
    struct B<'a> {
        name: &'a str,
        list: Vec<&'a str>,
        #[serde(borrow)]
        map: BTreeMap<&'a str, &'a str>,
    }
    let arena = rs_toml::Arena::new();
    let src = "name = 'n'\nlist = ['a', \"b\"]\n[map]\nk = 'v'\n";
    let doc = rs_toml::parse(src, &arena).unwrap();
    let b: B<'_> = rs_toml::serde::from_document(&doc).unwrap();
    assert_eq!(b.name, "n");
    assert_eq!(b.list, ["a", "b"]);
    assert_eq!(b.map["k"], "v");
}

#[test]
fn from_item_on_sub_table() {
    let arena = rs_toml::Arena::new();
    let doc = rs_toml::parse("[unix]\npath = '/s'", &arena).unwrap();
    let unix: Unix = rs_toml::serde::from_item(doc["unix"].item().unwrap()).unwrap();
    assert_eq!(unix.mode, 0o660);
}

#[test]
fn datetimes_are_strings() {
    #[derive(Debug, Deserialize)]
    struct D {
        at: String,
        day: String,
        time: String,
    }
    let d: D =
        rs_toml::serde::from_str("at = 1979-05-27T07:32:00Z\nday = 1979-05-27\ntime = 07:32:00")
            .unwrap();
    assert_eq!(d.at, "1979-05-27T07:32:00Z");
    assert_eq!(d.day, "1979-05-27");
    assert_eq!(d.time, "07:32:00");
}

#[test]
fn enums_unit_external_newtype_tuple_struct() {
    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    enum E {
        Plain,
        Wrapped(u8),
        Pair(u8, String),
        Named { x: i32 },
    }
    #[derive(Debug, Deserialize, PartialEq)]
    struct H {
        e: Vec<E>,
    }
    let h: H = rs_toml::serde::from_str(
        "e = ['plain', { wrapped = 3 }, { pair = [1, 'b'] }, { named = { x = -2 } }, { plain = {} }]",
    )
    .unwrap();
    assert_eq!(
        h.e,
        [
            E::Plain,
            E::Wrapped(3),
            E::Pair(1, "b".into()),
            E::Named { x: -2 },
            E::Plain
        ]
    );
    let src = "e = [{ wrapped = 3, pair = [] }]";
    assert!(err::<H>(src).span().is_some());
    let src = "e = [{ bogus = 1 }]";
    let e = err::<H>(src);
    assert!(e.message().starts_with("unknown variant, expected"), "{e}");
    assert!(!e.message().contains("bogus"), "{e}");
    assert_eq!(spanned(src, &e), "bogus");
}

#[test]
fn untagged_enum() {
    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(untagged)]
    enum Port {
        Number(u16),
        Named(String),
        Range { from: u16, to: u16 },
    }
    #[derive(Debug, Deserialize, PartialEq)]
    struct P {
        ports: Vec<Port>,
    }
    let p: P = rs_toml::serde::from_str("ports = [80, 'https', { from = 1, to = 2 }]").unwrap();
    assert_eq!(
        p.ports,
        [
            Port::Number(80),
            Port::Named("https".into()),
            Port::Range { from: 1, to: 2 }
        ]
    );
    let src = "ports = [80, true]";
    let e = err::<P>(src);
    assert_eq!(spanned(src, &e), "true");
}

#[test]
fn adjacently_tagged_enum() {
    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(tag = "t", content = "c")]
    enum A {
        One(u8),
        Two { s: String },
    }
    let a: Vec<A> = rs_toml::serde::from_str::<BTreeMap<String, Vec<A>>>(
        "a = [{ t = 'One', c = 1 }, { t = 'Two', c = { s = 'x' } }]",
    )
    .unwrap()
    .remove("a")
    .unwrap();
    assert_eq!(a, [A::One(1), A::Two { s: "x".into() }]);
}

#[test]
fn flatten() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Common {
        name: String,
        #[serde(default)]
        port: u16,
    }
    #[derive(Debug, Deserialize, PartialEq)]
    struct Outer {
        #[serde(flatten)]
        common: Common,
        #[serde(flatten)]
        rest: HashMap<String, i64>,
    }
    let o: Outer = rs_toml::serde::from_str("name = 'n'\nport = 9\nextra = 4").unwrap();
    assert_eq!(o.common.name, "n");
    assert_eq!(o.common.port, 9);
    assert_eq!(o.rest.get("extra"), Some(&4));
}

#[test]
fn newtype_unit_tuple_char_and_ignored() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Port(u16);
    #[derive(Debug, Deserialize, PartialEq)]
    struct Marker;
    #[derive(Debug, Deserialize, PartialEq)]
    struct T {
        port: Port,
        marker: Marker,
        unit: (),
        pair: (u8, bool),
        c: char,
        #[serde(skip)]
        skipped: u8,
    }
    #[derive(Debug, Deserialize, PartialEq)]
    struct Lenient {
        port: Port,
    }
    let t: T =
        rs_toml::serde::from_str("port = 1\nmarker = {}\nunit = {}\npair = [1, true]\nc = 'z'")
            .unwrap();
    assert_eq!(t.port, Port(1));
    assert_eq!(t.pair, (1, true));
    assert_eq!(t.c, 'z');
    assert_eq!(t.skipped, 0);
    // Unknown keys of any shape are ignored without deny_unknown_fields.
    let l: Lenient =
        rs_toml::serde::from_str("port = 2\n[other]\nx = [1, {a = 1}]\ny = 1979-05-27").unwrap();
    assert_eq!(l.port, Port(2));
    let src = "port = 1\nmarker = {}\nunit = {}\npair = [1, true, 3]\nc = 'z'";
    let e = err::<T>(src);
    assert!(e.message().starts_with("invalid length 3"), "{e}");
    assert_eq!(spanned(src, &e), "[1, true, 3]");
}

#[test]
fn maps_and_nested_sequences() {
    let m: BTreeMap<String, Vec<Vec<u8>>> =
        rs_toml::serde::from_str("a = [[1, 2], []]\nb = []").unwrap();
    assert_eq!(m["a"], [vec![1, 2], vec![]]);
    let m: HashMap<String, BTreeMap<String, bool>> =
        rs_toml::serde::from_str("[x]\ny = true\n[z]\n").unwrap();
    assert!(m["x"]["y"]);
    assert!(m["z"].is_empty());
}

#[test]
fn errors_are_serde_errors() {
    fn assert_error<E: serde::de::Error>() {}
    assert_error::<rs_toml::serde::Error>();
}

/// Deserializes `src` with rs-toml and with the `toml` crate and checks
/// both agree, including on whether it fails.
fn differential<T>(src: &str)
where
    T: serde::de::DeserializeOwned + PartialEq + Debug,
{
    let ours = rs_toml::serde::from_str::<T>(src);
    let theirs = toml::from_str::<T>(src);
    match (ours, theirs) {
        (Ok(a), Ok(b)) => assert_eq!(a, b, "{src}"),
        (Err(a), Err(b)) => {
            let b_span = b.span();
            // Both locate the problem; rs-toml's span lies inside or on
            // toml's, or both point at the same start.
            let a_span = a.span();
            assert!(
                a_span.is_some(),
                "rs-toml error without span: {a} / toml: {b}"
            );
            if let (Some(a), Some(b)) = (a_span, b_span) {
                assert!(
                    (a.start >= b.start && a.end <= b.end)
                        || (b.start >= a.start && b.end <= a.end),
                    "spans differ for {src:?}: rs-toml {a:?}, toml {b:?}"
                );
            }
        }
        (a, b) => panic!("disagree on {src:?}: rs-toml {a:?}, toml {b:?}"),
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Small {
    name: String,
    port: u16,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    ratio: Option<f64>,
    #[serde(default)]
    nested: Option<BTreeMap<String, i64>>,
}

#[test]
fn differential_against_toml_crate() {
    differential::<Config>(FULL);
    differential::<Config>("listen = 'x'");
    differential::<Config>("listen = 'x'\n[storage]\ntype = 'file'\npath = '/d'");
    differential::<Config>("listen = 'x'\nbogus = 1");
    differential::<Config>("listen = 'x'\nmlock = 3");
    differential::<Config>("listen = 'x'\n[limits]\nmax_connections = -1");
    differential::<Config>("listen = 'x'\n[[outbound]]\nurl = 'a'\nuse = ['jwt', 'nope']");
    differential::<Small>("name = 'a'\nport = 1\ntags = ['x', 'y']\nratio = 0.5");
    differential::<Small>("name = 'a'\nport = 1\n[nested]\na = 1\nb = -2");
    differential::<Small>("name = \"\\u00e9\\n\"\nport = 65535");
    differential::<Small>("name = 'a'\nport = 65536");
    differential::<Small>("name = 'a'\nport = 1\ntags = 'x'");
    differential::<Small>("name = 'a'\nport = 1\nextra = true");
    differential::<Small>("port = 1");
    differential::<Small>("name = 'a'\nport = 0x1F\ntags = []");
    differential::<BTreeMap<String, toml::Value>>(
        "a = 1\nb = 'two'\nc = [1.5, true]\n[d.e]\nf = { g = 'h' }\n",
    );
}
