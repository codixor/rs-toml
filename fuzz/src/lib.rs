//! Shared helpers for the rs-toml fuzz targets: a small, parser-neutral value
//! tree that both rs-toml and the `toml` crate convert into, so values can be
//! compared structurally.
//!
//! Normalisations (representation differences, not semantic ones):
//! - Floats compare by bit pattern, except that every NaN equals every other
//!   NaN (TOML `nan`, `+nan` and `-nan` are not required to keep a payload or
//!   sign).
//! - Times store `second` and `nanosecond` with omitted parts as `0`; the
//!   number of fractional digits written in the source is not compared.
//! - Integers are widened to `i128` (rs-toml accepts the full `i128` range;
//!   the `toml` crate only accepts `i64`; see [`Tree::has_wide_integer`]).

use std::collections::BTreeMap;

/// Parser-neutral TOML value.
#[derive(Debug, Clone)]
pub enum Tree {
    String(String),
    Integer(i128),
    Float(f64),
    Boolean(bool),
    Datetime(Datetime),
    Array(Vec<Tree>),
    Table(BTreeMap<String, Tree>),
}

/// Parser-neutral datetime: each part is present or absent as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Datetime {
    pub date: Option<(u16, u8, u8)>,
    /// `(hour, minute, second, nanosecond)`, omitted parts normalised to 0.
    pub time: Option<(u8, u8, u8, u32)>,
    /// `None` = local, `Some(None)` = `Z`, `Some(Some(m))` = offset minutes.
    pub offset: Option<Option<i16>>,
}

impl PartialEq for Tree {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Tree::String(a), Tree::String(b)) => a == b,
            (Tree::Integer(a), Tree::Integer(b)) => a == b,
            (Tree::Float(a), Tree::Float(b)) => {
                (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
            }
            (Tree::Boolean(a), Tree::Boolean(b)) => a == b,
            (Tree::Datetime(a), Tree::Datetime(b)) => a == b,
            (Tree::Array(a), Tree::Array(b)) => a == b,
            (Tree::Table(a), Tree::Table(b)) => a == b,
            _ => false,
        }
    }
}

impl Tree {
    /// True if the tree holds an integer outside the `i64` range, which
    /// rs-toml accepts and the `toml` crate rejects by design.
    pub fn has_wide_integer(&self) -> bool {
        match self {
            Tree::Integer(i) => i64::try_from(*i).is_err(),
            Tree::Array(items) => items.iter().any(Tree::has_wide_integer),
            Tree::Table(map) => map.values().any(Tree::has_wide_integer),
            _ => false,
        }
    }
}

/// Converts an rs-toml item into a [`Tree`].
pub fn from_rs_toml(item: &rs_toml::Item<'_>) -> Tree {
    use rs_toml::Value;
    match item.value() {
        Value::String(s) => Tree::String((*s).to_owned()),
        Value::Integer(i) => Tree::Integer(i.as_i128()),
        Value::Float(f) => Tree::Float(*f),
        Value::Boolean(b) => Tree::Boolean(*b),
        Value::DateTime(dt) => Tree::Datetime(Datetime {
            date: dt.date().map(|d| (d.year, d.month, d.day)),
            time: dt.time().map(|t| (t.hour, t.minute, t.second, t.nanosecond)),
            offset: dt.offset().map(|o| match o {
                rs_toml::TimeOffset::Z => None,
                rs_toml::TimeOffset::Custom { minutes } => Some(minutes),
            }),
        }),
        Value::Array(array) => Tree::Array(array.iter().map(from_rs_toml).collect()),
        Value::Table(table) => from_rs_toml_table(table),
    }
}

/// Converts an rs-toml table into a [`Tree::Table`].
pub fn from_rs_toml_table(table: &rs_toml::Table<'_>) -> Tree {
    Tree::Table(
        table
            .iter()
            .map(|(key, value)| (key.as_str().to_owned(), from_rs_toml(value)))
            .collect(),
    )
}

/// Converts a `toml` crate value into a [`Tree`].
pub fn from_toml(value: &toml::Value) -> Tree {
    match value {
        toml::Value::String(s) => Tree::String(s.clone()),
        toml::Value::Integer(i) => Tree::Integer(i128::from(*i)),
        toml::Value::Float(f) => Tree::Float(*f),
        toml::Value::Boolean(b) => Tree::Boolean(*b),
        toml::Value::Datetime(dt) => Tree::Datetime(Datetime {
            date: dt.date.map(|d| (d.year, d.month, d.day)),
            time: dt.time.map(|t| {
                (
                    t.hour,
                    t.minute,
                    t.second.unwrap_or(0),
                    t.nanosecond.unwrap_or(0),
                )
            }),
            offset: dt.offset.map(|o| match o {
                toml::value::Offset::Z => None,
                toml::value::Offset::Custom { minutes } => Some(minutes),
            }),
        }),
        toml::Value::Array(items) => Tree::Array(items.iter().map(from_toml).collect()),
        toml::Value::Table(table) => from_toml_table(table),
    }
}

/// Converts a `toml` crate table into a [`Tree::Table`].
pub fn from_toml_table(table: &toml::Table) -> Tree {
    Tree::Table(
        table
            .iter()
            .map(|(key, value)| (key.clone(), from_toml(value)))
            .collect(),
    )
}
