//! toml-test encoder: reads the toml-test JSON description on stdin and
//! writes TOML produced by the rs-toml emitter on stdout.

use rs_toml::{Arena, Array, DateTime, Formatting, Item, Key, Table};
use serde_json::{Map, Value as Json};
use std::io::Read;

type Result<T> = std::result::Result<T, String>;

fn parse_float(text: &str) -> Result<f64> {
    match text {
        "nan" | "+nan" | "-nan" => Ok(f64::NAN),
        "inf" | "+inf" => Ok(f64::INFINITY),
        "-inf" => Ok(f64::NEG_INFINITY),
        _ => text.parse().map_err(|e| format!("bad float {text:?}: {e}")),
    }
}

fn scalar<'a>(obj: &Map<String, Json>, arena: &'a Arena) -> Result<Item<'a>> {
    let (Some(Json::String(kind)), Some(Json::String(value))) = (obj.get("type"), obj.get("value"))
    else {
        return Err("scalar must have string `type` and `value`".to_owned());
    };
    let item = match kind.as_str() {
        "string" => Item::string(arena.alloc_str(value)),
        "integer" => Item::from(
            value
                .parse::<i128>()
                .map_err(|e| format!("bad integer {value:?}: {e}"))?,
        ),
        "float" => Item::from(parse_float(value)?),
        "bool" => Item::from(match value.as_str() {
            "true" => true,
            "false" => false,
            _ => return Err(format!("bad bool {value:?}")),
        }),
        "datetime" | "datetime-local" | "date-local" | "time-local" => Item::from(
            value
                .parse::<DateTime>()
                .map_err(|e| format!("bad {kind} {value:?}: {e}"))?,
        ),
        _ => return Err(format!("unknown type {kind:?}")),
    };
    Ok(item)
}

/// A JSON object is a scalar description iff it has exactly the keys
/// `type` and `value`, both strings.
fn is_scalar(obj: &Map<String, Json>) -> bool {
    obj.len() == 2 && matches!(obj.get("type"), Some(Json::String(_)))
        && matches!(obj.get("value"), Some(Json::String(_)))
}

fn table<'a>(obj: &Map<String, Json>, arena: &'a Arena) -> Result<Table<'a>> {
    let mut table = Table::new();
    for (key, value) in obj {
        let key = Key::new(arena.alloc_str(key));
        table.insert_unique(key, convert(value, arena)?, arena);
    }
    Ok(table)
}

fn convert<'a>(json: &Json, arena: &'a Arena) -> Result<Item<'a>> {
    match json {
        Json::Object(obj) if is_scalar(obj) => scalar(obj, arena),
        Json::Object(obj) => Ok(table(obj, arena)?.into_item()),
        Json::Array(values) => {
            let mut array = Array::new();
            for value in values {
                array.push(convert(value, arena)?, arena);
            }
            Ok(array.into_item())
        }
        other => Err(format!("unexpected JSON value {other}")),
    }
}

fn run() -> Result<String> {
    let mut buffer = Vec::new();
    std::io::stdin()
        .read_to_end(&mut buffer)
        .map_err(|e| format!("failed to read stdin: {e}"))?;
    let json: Json =
        serde_json::from_slice(&buffer).map_err(|e| format!("invalid JSON input: {e}"))?;
    let Json::Object(obj) = json else {
        return Err("expected a top-level JSON object".to_owned());
    };
    let arena = Arena::new();
    let table = table(&obj, &arena)?;
    let bytes = Formatting::default().format_table_to_bytes(table, &arena);
    String::from_utf8(bytes).map_err(|e| format!("emitter produced invalid UTF-8: {e}"))
}

fn main() {
    match run() {
        Ok(text) => print!("{text}"),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
