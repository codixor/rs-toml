//! toml-test decoder: reads TOML on stdin, writes the toml-test JSON
//! description on stdout. Exits 1 on invalid TOML.

use rs_toml::{DateTime, Item, Value};
use serde_json::{Map, Value as Json};
use std::io::Read;
use std::mem::MaybeUninit;

fn datetime_kind(dt: &DateTime) -> &'static str {
    match (dt.date().is_some(), dt.time().is_some(), dt.offset().is_some()) {
        (true, true, true) => "datetime",
        (true, true, false) => "datetime-local",
        (true, false, false) => "date-local",
        (false, true, false) => "time-local",
        // rs_toml::parse never produces other combinations.
        _ => "invalid-datetime",
    }
}

fn format_float(value: f64) -> String {
    if value.is_nan() {
        "nan".to_owned()
    } else if value.is_infinite() {
        if value.is_sign_negative() { "-inf" } else { "inf" }.to_owned()
    } else {
        // Debug keeps a fractional part / exponent and round-trips exactly.
        format!("{value:?}")
    }
}

fn scalar(kind: &str, value: String) -> Json {
    let mut obj = Map::new();
    obj.insert("type".to_owned(), Json::String(kind.to_owned()));
    obj.insert("value".to_owned(), Json::String(value));
    Json::Object(obj)
}

fn describe(item: &Item<'_>) -> Json {
    match item.value() {
        Value::String(value) => scalar("string", (*value).to_owned()),
        Value::Integer(value) => scalar("integer", value.to_string()),
        Value::Float(value) => scalar("float", format_float(*value)),
        Value::Boolean(value) => scalar("bool", value.to_string()),
        Value::DateTime(value) => {
            let mut buf = MaybeUninit::uninit();
            scalar(datetime_kind(value), value.format(&mut buf).to_owned())
        }
        Value::Array(array) => Json::Array(array.iter().map(describe).collect()),
        Value::Table(table) => {
            let mut obj = Map::new();
            for (key, value) in table {
                obj.insert(key.as_str().to_owned(), describe(value));
            }
            Json::Object(obj)
        }
    }
}

fn main() {
    let mut buffer = Vec::new();
    if let Err(err) = std::io::stdin().read_to_end(&mut buffer) {
        eprintln!("failed to read stdin: {err}");
        std::process::exit(1);
    }
    let Ok(content) = std::str::from_utf8(&buffer) else {
        eprintln!("input is not valid UTF-8");
        std::process::exit(1);
    };
    let arena = rs_toml::Arena::new();
    match rs_toml::parse(content, &arena) {
        Ok(doc) => {
            let json = describe(&doc.into_item());
            println!("{json}");
        }
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
