#![allow(clippy::manual_map)]
#[cfg(test)]
#[path = "../tests/unit/value.rs"]
mod tests;

pub(crate) mod array;
pub(crate) mod table;
mod to_toml;
use crate::error::{Error, ErrorKind};
use crate::item::table::TableIndex;
use crate::{DateTime, Span, Table};
use std::fmt;

pub(crate) use crate::raw::item::NONE;
pub use crate::raw::item::{Item, MaybeItem};
pub use array::Array;
pub(crate) use array::InternalArray;
pub(crate) use table::InnerTable;

pub(crate) const TAG_MASK: u32 = 0x7;
pub(crate) const TAG_SHIFT: u32 = 3;

pub(crate) const TAG_STRING: u32 = 0;
pub(crate) const TAG_INTEGER: u32 = 1;
pub(crate) const TAG_FLOAT: u32 = 2;
pub(crate) const TAG_BOOLEAN: u32 = 3;
pub(crate) const TAG_DATETIME: u32 = 4;
pub(crate) const TAG_TABLE: u32 = 5;
pub(crate) const TAG_ARRAY: u32 = 6;

// Only set in maybe item
pub(crate) const TAG_NONE: u32 = 7;

/// 3-bit state field in `end_and_flag` encoding container kind and sub-state.
/// Bit 2 set → table, bits 1:0 == 01 → array. Allows dispatch without
/// reading `start_and_tag`.
pub(crate) const FLAG_MASK: u32 = 0x7;
pub(crate) const FLAG_SHIFT: u32 = 3;

pub(crate) const FLAG_NONE: u32 = 0;
pub(crate) const FLAG_ARRAY: u32 = 2;
pub(crate) const FLAG_AOT: u32 = 3;
pub(crate) const FLAG_TABLE: u32 = 4;
pub(crate) const FLAG_DOTTED: u32 = 5;
pub(crate) const FLAG_HEADER: u32 = 6;
pub(crate) const FLAG_FROZEN: u32 = 7;

/// Bit 31 of `end_and_flag`: when set, the metadata is in format-hints mode
/// (constructed programmatically); when clear, it is in span mode (from parser).
pub(crate) const HINTS_BIT: u32 = 1 << 31;
/// Bit 26 of `end_and_flag`: when set in hints mode, defers style decisions
/// to normalization time. Resolved based on content heuristics.
pub(crate) const AUTO_STYLE_BIT: u32 = 1 << 26;
/// Value bits (above TAG_SHIFT) all set = "not projected".
pub(crate) const NOT_PROJECTED: u32 = !(TAG_MASK); // 0xFFFF_FFF8

/// The kind of a TOML table, distinguishing how it was defined in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableStyle {
    /// Structural parent with no explicit header (`FLAG_TABLE`).
    Implicit,
    /// Created by dotted keys, e.g. `a.b.c = 1` (`FLAG_DOTTED`).
    Dotted,
    /// Explicit `[section]` header (`FLAG_HEADER`).
    Header,
    /// Inline `{ }` table (`FLAG_FROZEN`).
    Inline,
}

/// The kind of a TOML array, distinguishing inline arrays from arrays of tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrayStyle {
    /// Inline `[1, 2, 3]` array (`FLAG_ARRAY`).
    Inline,
    /// Array of tables `[[section]]` (`FLAG_AOT`).
    Header,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct PackedI128 {
    value: i128,
}

/// A TOML integer value.
///
/// This is a storage type that supports the full range of `i128`.
/// Convert to a primitive with [`as_i128`](Self::as_i128),
/// [`as_i64`](Self::as_i64), or [`as_u64`](Self::as_u64), perform
/// your arithmetic there, then convert back with `From`.
#[repr(align(8))]
#[derive(Clone, Copy)]
pub struct Integer {
    value: PackedI128,
}

impl Integer {
    /// Zero, usable in `const`/`static` initializers.
    pub(crate) const ZERO: Integer = Integer {
        value: PackedI128 { value: 0 },
    };

    /// Returns the value as an `i128`.
    #[inline]
    pub fn as_i128(&self) -> i128 {
        let copy = *self;
        copy.value.value
    }

    /// Returns the value as an `f64`, which may be lossy for large integers.
    #[inline]
    pub fn as_f64(&self) -> f64 {
        let copy = *self;
        copy.value.value as f64
    }

    /// Returns the value as an `i64`, or [`None`] if it does not fit.
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        i64::try_from(self.as_i128()).ok()
    }

    /// Returns the value as a `u64`, or [`None`] if it does not fit.
    #[inline]
    pub fn as_u64(&self) -> Option<u64> {
        u64::try_from(self.as_i128()).ok()
    }
}

impl From<i128> for Integer {
    #[inline]
    fn from(v: i128) -> Self {
        Self {
            value: PackedI128 { value: v },
        }
    }
}

impl From<i64> for Integer {
    #[inline]
    fn from(v: i64) -> Self {
        Self::from(v as i128)
    }
}

impl From<u64> for Integer {
    #[inline]
    fn from(v: u64) -> Self {
        Self::from(v as i128)
    }
}

impl From<i32> for Integer {
    #[inline]
    fn from(v: i32) -> Self {
        Self::from(v as i128)
    }
}

impl From<u32> for Integer {
    #[inline]
    fn from(v: u32) -> Self {
        Self::from(v as i128)
    }
}

impl PartialEq for Integer {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_i128() == other.as_i128()
    }
}

impl Eq for Integer {}

impl fmt::Debug for Integer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_i128().fmt(f)
    }
}

impl fmt::Display for Integer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_i128().fmt(f)
    }
}

/// Discriminant for the TOML value types stored in an [`Item`].
///
/// Obtained via [`Item::kind`].
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(unused)]
pub enum Kind {
    String = 0,
    Integer = 1,
    Float = 2,
    Boolean = 3,
    DateTime = 4,
    Table = 5,
    Array = 6,
}

impl std::fmt::Debug for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl Kind {
    /// Returns the TOML type name as a lowercase string (e.g. `"string"`, `"table"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::String => "string",
            Kind::Integer => "integer",
            Kind::Float => "float",
            Kind::Boolean => "boolean",
            Kind::Array => "array",
            Kind::Table => "table",
            Kind::DateTime => "datetime",
        }
    }
}

/// Borrowed view into an [`Item`] for pattern matching.
///
/// Obtained via [`Item::value`].
///
/// # Examples
///
/// ```
/// use rs_toml::{Arena, Value};
///
/// let arena = Arena::new();
/// let table = rs_toml::parse("n = 10", &arena).unwrap();
/// match table["n"].item().unwrap().value() {
///     Value::Integer(i) => assert_eq!(i.as_i128(), 10),
///     _ => panic!("expected integer"),
/// }
/// ```
#[derive(Debug)]
pub enum Value<'a, 'de> {
    /// A string value.
    String(&'a &'de str),
    /// An integer value.
    Integer(&'a Integer),
    /// A floating-point value.
    Float(&'a f64),
    /// A boolean value.
    Boolean(&'a bool),
    /// A datetime value.
    DateTime(&'a DateTime),
    /// A table value.
    Table(&'a Table<'de>),
    /// An array value.
    Array(&'a Array<'de>),
}

/// Mutable view into an [`Item`] for pattern matching.
///
/// Obtained via [`Item::value_mut`].
pub enum ValueMut<'a, 'de> {
    /// A string value.
    String(&'a mut &'de str),
    /// An integer value.
    Integer(&'a mut Integer),
    /// A floating-point value.
    Float(&'a mut f64),
    /// A boolean value.
    Boolean(&'a mut bool),
    /// A datetime value (read-only, datetime fields are not mutable).
    DateTime(&'a DateTime),
    /// A table value.
    Table(&'a mut Table<'de>),
    /// An array value.
    Array(&'a mut Array<'de>),
}

impl<'de> Item<'de> {
    /// Creates an "expected X, found Y" error using this value's type and span.
    #[inline]
    pub fn expected(&self, expected: &'static &'static str) -> Error {
        Error::new(
            ErrorKind::Wanted {
                expected,
                found: self.type_str(),
            },
            self.span_unchecked(),
        )
    }

    /// Takes a string value and parses it via [`std::str::FromStr`].
    ///
    /// Returns an error if the value is not a string or parsing fails.
    #[inline]
    pub fn parse<T>(&self) -> Result<T, Error>
    where
        T: std::str::FromStr,
        <T as std::str::FromStr>::Err: std::fmt::Display,
    {
        let Some(s) = self.as_str() else {
            return Err(self.expected(&"a string"));
        };
        match s.parse() {
            Ok(v) => Ok(v),
            Err(err) => Err(Error::custom(err, self.span_unchecked())),
        }
    }
}

impl fmt::Debug for Item<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value() {
            Value::String(s) => s.fmt(f),
            Value::Integer(i) => i.fmt(f),
            Value::Float(v) => v.fmt(f),
            Value::Boolean(b) => b.fmt(f),
            Value::Array(a) => a.fmt(f),
            Value::Table(t) => t.fmt(f),
            Value::DateTime(m) => {
                let mut buf = std::mem::MaybeUninit::uninit();
                f.write_str(m.format(&mut buf))
            }
        }
    }
}

/// A TOML table key with its source span.
///
/// Keys appear as the first element in `(`[`Key`]`, `[`Item`]`)` entry pairs
/// when iterating over a [`Table`].
#[derive(Copy, Clone)]
pub struct Key<'de> {
    /// The key name.
    pub name: &'de str,
    /// The byte-offset span of the key in the source document.
    pub span: Span,
}

impl<'de> Key<'de> {
    /// Creates a key with no source span.
    ///
    /// Use this when constructing tables programmatically via
    /// [`Table::insert`].
    pub fn new(value: &'de str) -> Self {
        Self {
            name: value,
            span: Span::default(),
        }
    }
    /// Returns the key name as a string slice.
    pub fn as_str(&self) -> &'de str {
        self.name
    }
}

impl<'de> From<&'de str> for Key<'de> {
    fn from(value: &'de str) -> Self {
        Self::new(value)
    }
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<Key<'_>>() == 24);

impl std::borrow::Borrow<str> for Key<'_> {
    fn borrow(&self) -> &str {
        self.name
    }
}

impl fmt::Debug for Key<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

impl fmt::Display for Key<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

impl Ord for Key<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(other.name)
    }
}

impl PartialOrd for Key<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Key<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(other.name)
    }
}

impl Eq for Key<'_> {}

impl std::hash::Hash for Key<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

pub(crate) fn equal_items(a: &Item<'_>, b: &Item<'_>, index: Option<&TableIndex<'_>>) -> bool {
    match (a.value(), b.value()) {
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Integer(a), Value::Integer(b)) => a == b,
        (Value::Float(af), Value::Float(bf)) => {
            if af.is_nan() && bf.is_nan() {
                af.is_sign_negative() == bf.is_sign_negative()
            } else {
                af.to_bits() == bf.to_bits()
            }
        }
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::DateTime(a), Value::DateTime(b)) => a == b,
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(a, b)| equal_items(a, b, index))
        }
        (Value::Table(tab_a), Value::Table(tab_b)) => {
            if tab_a.len() != tab_b.len() {
                return false;
            }
            for (key, val_a) in tab_a {
                let Some((_, val_b)) = tab_b.value.get_entry_with_maybe_index(key.name, index)
                else {
                    return false;
                };
                if !equal_items(val_a, val_b, index) {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

impl<'de> PartialEq for Item<'de> {
    fn eq(&self, other: &Self) -> bool {
        equal_items(self, other, None)
    }
}

impl<'de> std::ops::Index<&str> for Item<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: &str) -> &Self::Output {
        if let Some(table) = self.as_table()
            && let Some(item) = table.get(index)
        {
            return MaybeItem::from_ref(item);
        }
        &NONE
    }
}

impl<'de> std::ops::Index<usize> for Item<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        if let Some(arr) = self.as_array()
            && let Some(item) = arr.get(index)
        {
            return MaybeItem::from_ref(item);
        }
        &NONE
    }
}

impl<'de> std::ops::Index<&str> for MaybeItem<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: &str) -> &Self::Output {
        if let Some(table) = self.as_table()
            && let Some(item) = table.get(index)
        {
            return MaybeItem::from_ref(item);
        }
        &NONE
    }
}

impl<'de> std::ops::Index<usize> for MaybeItem<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        if let Some(arr) = self.as_array()
            && let Some(item) = arr.get(index)
        {
            return MaybeItem::from_ref(item);
        }
        &NONE
    }
}

impl fmt::Debug for MaybeItem<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.item() {
            Some(item) => item.fmt(f),
            None => f.write_str("None"),
        }
    }
}
