//! Serde deserialization from parsed TOML.
//!
//! Enabled by the `serde` feature flag. [`from_str`] parses a document and
//! deserializes any type implementing [`serde::Deserialize`], a drop-in for
//! `toml::from_str`:
//!
//! ```
//! #[derive(Debug, serde::Deserialize)]
//! #[serde(deny_unknown_fields)]
//! struct Config {
//!     name: String,
//!     port: u16,
//!     #[serde(default)]
//!     tags: Vec<String>,
//! }
//!
//! let config: Config = rs_toml::serde::from_str("name = 'app'\nport = 8080").unwrap();
//! assert_eq!(config.port, 8080);
//!
//! let err = rs_toml::serde::from_str::<Config>("name = 'app'\nport = 70000").unwrap_err();
//! assert_eq!(err.span(), Some(20..25));
//! ```
//!
//! The crate-level [`crate::from_str`] is a different function: it uses the
//! [`FromToml`](crate::FromToml) trait, not serde.
//!
//! [`from_document`] and [`from_item`] deserialize from a document parsed
//! with [`parse`](crate::parse), so the result can borrow strings
//! (`&'de str`) from the source text or the [`Arena`].
//!
//! # Data model
//!
//! - Integers are range checked against the target type; an out-of-range
//!   value is an error, never wrapped or truncated. Integers also
//!   deserialize into `f32`/`f64`.
//! - Datetimes are presented as strings in RFC 3339 form (as rs-toml
//!   formats them), so they deserialize into `String` or any type that
//!   parses one. `toml::value::Datetime` is not supported.
//! - TOML has no null: a missing key is `None` for an `Option` field, and a
//!   present value is always `Some`.
//! - Enums: a string selects a unit variant; a table with exactly one key
//!   selects the variant named by the key, with the value as its content.
//!   Internally tagged and untagged enums and `#[serde(flatten)]` work
//!   through [`serde::Deserializer::deserialize_any`].
//! - Unit and unit structs accept an empty table.
//!
//! # Errors
//!
//! [`Error`] carries the byte span of the offending key or value, so a
//! caller can report a line and column without quoting the source. An error
//! raised without a position (such as `missing field`) takes the span of the
//! innermost enclosing table or value.

use std::fmt;
use std::mem::MaybeUninit;
use std::ops::Range;

use ::serde::de::value::BorrowedStrDeserializer;
use ::serde::de::{
    self, DeserializeOwned, DeserializeSeed, Deserializer, EnumAccess, IntoDeserializer, MapAccess,
    SeqAccess, Unexpected, VariantAccess, Visitor,
};
use ::serde::forward_to_deserialize_any;

use crate::item::Value;
use crate::{Arena, Document, Item, Key, Span};

/// An error from serde deserialization, with the source span where it
/// occurred.
///
/// The message follows serde's conventions (`missing field `x``,
/// `unknown field `x`, expected ...`, `invalid type: ...`). Messages built
/// by serde for invalid types or values may quote the offending value;
/// use [`span`](Self::span) to locate the error without echoing source text.
#[derive(Clone, PartialEq, Eq)]
pub struct Error {
    message: Box<str>,
    span: Option<Span>,
}

impl Error {
    /// Returns the error message, without position information.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the byte range in the source document where the error
    /// occurred, if known.
    pub fn span(&self) -> Option<Range<usize>> {
        self.span.map(Span::range)
    }

    fn from_parse(error: &crate::Error, source: &str) -> Self {
        let span = error.span();
        Error {
            message: error.message(source).into(),
            span: (!span.is_empty()).then_some(span),
        }
    }

    /// Attaches `span` if the error has none yet.
    fn at(mut self, span: Span) -> Self {
        if self.span.is_none() && !span.is_empty() {
            self.span = Some(span);
        }
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("message", &self.message)
            .field("span", &self.span())
            .finish()
    }
}

impl std::error::Error for Error {}

impl de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error {
            message: msg.to_string().into(),
            span: None,
        }
    }
}

/// Parses `source` and deserializes it into `T` with serde.
///
/// # Errors
///
/// Returns an [`Error`] for a TOML syntax error or a value that does not
/// match `T`, with the span of the offending key or value.
pub fn from_str<T: DeserializeOwned>(source: &str) -> Result<T, Error> {
    let arena = Arena::new();
    let doc = crate::parse(source, &arena).map_err(|e| Error::from_parse(&e, source))?;
    from_document(&doc)
}

/// Deserializes the root table of a parsed [`Document`] into `T`.
///
/// `T` may borrow strings for the document's lifetime.
///
/// ```
/// #[derive(serde::Deserialize)]
/// struct Named<'a> {
///     name: &'a str,
/// }
///
/// let arena = rs_toml::Arena::new();
/// let doc = rs_toml::parse("name = 'borrowed'", &arena).unwrap();
/// let named: Named<'_> = rs_toml::serde::from_document(&doc).unwrap();
/// assert_eq!(named.name, "borrowed");
/// ```
///
/// # Errors
///
/// Returns an [`Error`] when the document does not match `T`.
pub fn from_document<'de, T: de::Deserialize<'de>>(doc: &Document<'de>) -> Result<T, Error> {
    from_item(doc.table().as_item())
}

/// Deserializes one parsed [`Item`] into `T`.
///
/// # Errors
///
/// Returns an [`Error`] when the item does not match `T`.
pub fn from_item<'de, T: de::Deserialize<'de>>(item: &Item<'de>) -> Result<T, Error> {
    T::deserialize(ItemDeserializer::new(item))
}

/// Deepest container nesting [`ItemDeserializer`] descends into. Parsed
/// documents never exceed it (the parser enforces the same limit); it
/// guards trees built or modified through the mutable API.
const MAX_DEPTH: u16 = crate::parser::MAX_RECURSION_DEPTH.unsigned_abs();

/// A [`serde::Deserializer`] over a parsed [`Item`].
///
/// Descending into containers nested deeper than the parser's nesting
/// limit returns an [`Error`] instead of recursing further.
#[derive(Clone, Copy)]
pub struct ItemDeserializer<'a, 'de> {
    item: &'a Item<'de>,
    /// Nesting depth of `item`; the item passed to [`new`](Self::new) is 0.
    depth: u16,
}

impl<'a, 'de> ItemDeserializer<'a, 'de> {
    /// Creates a deserializer for `item`.
    pub fn new(item: &'a Item<'de>) -> Self {
        Self { item, depth: 0 }
    }

    /// A deserializer for `item`, a direct child of the current container.
    fn child<'b>(&self, item: &'b Item<'de>) -> ItemDeserializer<'b, 'de> {
        ItemDeserializer {
            item,
            depth: self.depth.saturating_add(1),
        }
    }

    /// Fails if this container is nested deeper than [`MAX_DEPTH`].
    fn check_depth(&self) -> Result<(), Error> {
        if self.depth > MAX_DEPTH {
            Err(Error {
                message: "max recursion depth exceeded".into(),
                span: None,
            }
            .at(self.item.span()))
        } else {
            Ok(())
        }
    }
}

fn unexpected<'a>(item: &'a Item<'_>) -> Unexpected<'a> {
    match item.value() {
        Value::String(s) => Unexpected::Str(s),
        Value::Integer(i) => match i.as_i64() {
            Some(v) => Unexpected::Signed(v),
            None => match i.as_u64() {
                Some(v) => Unexpected::Unsigned(v),
                None => Unexpected::Other("integer"),
            },
        },
        Value::Float(f) => Unexpected::Float(*f),
        Value::Boolean(b) => Unexpected::Bool(*b),
        Value::DateTime(_) => Unexpected::Other("datetime"),
        Value::Array(_) => Unexpected::Seq,
        Value::Table(_) => Unexpected::Map,
    }
}

fn is_empty_table(item: &Item<'_>) -> bool {
    item.as_table().is_some_and(crate::Table::is_empty)
}

impl<'de> Deserializer<'de> for ItemDeserializer<'_, 'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let span = self.item.span();
        let result = match self.item.value() {
            Value::String(s) => visitor.visit_borrowed_str(*s),
            Value::Integer(i) => {
                let v = i.as_i128();
                if let Ok(v) = i64::try_from(v) {
                    visitor.visit_i64(v)
                } else if let Ok(v) = u64::try_from(v) {
                    visitor.visit_u64(v)
                } else {
                    visitor.visit_i128(v)
                }
            }
            Value::Float(f) => visitor.visit_f64(*f),
            Value::Boolean(b) => visitor.visit_bool(*b),
            Value::DateTime(dt) => {
                let mut buf = MaybeUninit::uninit();
                visitor.visit_str(dt.format(&mut buf))
            }
            Value::Array(array) => {
                self.check_depth()?;
                let mut seq = SeqDeserializer {
                    iter: array.as_slice().iter(),
                    count: 0,
                    parent: self,
                };
                visitor.visit_seq(&mut seq).and_then(|value| seq.end(value))
            }
            Value::Table(table) => {
                self.check_depth()?;
                visitor.visit_map(MapDeserializer {
                    iter: table.entries().iter(),
                    value: None,
                    parent: self,
                })
            }
        };
        result.map_err(|e| e.at(span))
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_some(self)
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if is_empty_table(self.item) {
            visitor.visit_unit()
        } else {
            self.deserialize_any(visitor)
        }
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor
            .visit_newtype_struct(self)
            .map_err(|e| e.at(self.item.span()))
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        let span = self.item.span();
        let result = match self.item.value() {
            Value::String(s) => visitor.visit_enum(BorrowedStrDeserializer::new(*s)),
            Value::Table(table) if table.len() == 1 => {
                self.check_depth()?;
                let (key, value) = &table.entries()[0];
                visitor.visit_enum(EnumDeserializer {
                    key,
                    value: self.child(value),
                })
            }
            _ => Err(de::Error::invalid_type(
                unexpected(self.item),
                &"a string or a table with a single key",
            )),
        };
        result.map_err(|e| e.at(span))
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf seq tuple tuple_struct map struct identifier
    }
}

struct SeqDeserializer<'a, 'de> {
    iter: std::slice::Iter<'a, Item<'de>>,
    count: usize,
    /// The array being read.
    parent: ItemDeserializer<'a, 'de>,
}

impl SeqDeserializer<'_, '_> {
    /// Rejects elements the visitor left unread, as serde's own
    /// deserializers do.
    fn end<T>(&self, value: T) -> Result<T, Error> {
        let remaining = self.iter.len();
        if remaining == 0 {
            Ok(value)
        } else {
            Err(de::Error::invalid_length(
                self.count + remaining,
                &ExpectedInSeq(self.count),
            ))
        }
    }
}

struct ExpectedInSeq(usize);

impl de::Expected for ExpectedInSeq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 1 {
            f.write_str("1 element in array")
        } else {
            write!(f, "{} elements in array", self.0)
        }
    }
}

impl<'de> SeqAccess<'de> for SeqDeserializer<'_, 'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        let Some(item) = self.iter.next() else {
            return Ok(None);
        };
        self.count += 1;
        seed.deserialize(self.parent.child(item))
            .map(Some)
            .map_err(|e| e.at(item.span()))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

struct MapDeserializer<'a, 'de> {
    iter: std::slice::Iter<'a, (Key<'de>, Item<'de>)>,
    value: Option<&'a Item<'de>>,
    /// The table being read.
    parent: ItemDeserializer<'a, 'de>,
}

impl<'de> MapAccess<'de> for MapDeserializer<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        let Some((key, value)) = self.iter.next() else {
            return Ok(None);
        };
        self.value = Some(value);
        seed.deserialize(KeyDeserializer { key: *key }).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        let Some(item) = self.value.take() else {
            return Err(de::Error::custom("value requested before its key"));
        };
        seed.deserialize(self.parent.child(item))
            .map_err(|e| e.at(item.span()))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

/// Deserializes a table key: a string, a unit variant name or a field name.
struct KeyDeserializer<'de> {
    key: Key<'de>,
}

impl<'de> Deserializer<'de> for KeyDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor
            .visit_borrowed_str(self.key.name)
            .map_err(|e: Error| e.at(self.key.span))
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let span = self.key.span;
        visitor.visit_some(self).map_err(|e| e.at(span))
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        let span = self.key.span;
        visitor.visit_newtype_struct(self).map_err(|e| e.at(span))
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor
            .visit_enum(self.key.name.into_deserializer())
            .map_err(|e: Error| e.at(self.key.span))
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}

/// An externally tagged enum: a table with one key naming the variant.
struct EnumDeserializer<'a, 'de> {
    key: &'a Key<'de>,
    value: ItemDeserializer<'a, 'de>,
}

impl<'a, 'de> EnumAccess<'de> for EnumDeserializer<'a, 'de> {
    type Error = Error;
    type Variant = VariantDeserializer<'a, 'de>;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Error> {
        let variant = seed.deserialize(KeyDeserializer { key: *self.key })?;
        Ok((variant, VariantDeserializer { value: self.value }))
    }
}

struct VariantDeserializer<'a, 'de> {
    value: ItemDeserializer<'a, 'de>,
}

impl<'de> VariantAccess<'de> for VariantDeserializer<'_, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        let item = self.value.item;
        if is_empty_table(item) {
            Ok(())
        } else {
            let error: Error = de::Error::invalid_type(unexpected(item), &"an empty table");
            Err(error.at(item.span()))
        }
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        let span = self.value.item.span();
        seed.deserialize(self.value).map_err(|e| e.at(span))
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, Error> {
        self.value.deserialize_seq(visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.value.deserialize_map(visitor)
    }
}
