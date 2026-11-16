//! Packed value layout: the `Item` payload union, its metadata word and the
//! `Table`/`Array`/`MaybeItem` views that share `Item`'s layout.
//!
//! # Invariant
//!
//! The tag bits of an [`ItemMetadata`] name the active field of the payload
//! (`TAG_STRING` -> `string`, ..., `TAG_TABLE` -> `table`, `TAG_ARRAY` ->
//! `array`; `TAG_NONE` only in the `NONE` sentinel). The tag is written only
//! by the constructors in this module and never changes afterwards: metadata
//! fields are private, mutation goes through [`MetaMut`], which preserves the
//! tag, and no `&mut ItemMetadata` escapes this module. A `Table` always has
//! `TAG_TABLE` and an `Array` `TAG_ARRAY`, so the layout casts between them
//! and `Item` are sound. Every union read checks the tag first.

use crate::arena::Arena;
use crate::item::*;
use crate::{DateTime, Span};
use std::mem::ManuallyDrop;

/// Packed 8-byte metadata for `Item`, `Table`, and `Array`.
///
/// Two variants discriminated by bit 31 of `end_and_flag`:
///
/// **Span variant** (bit 31 = 0): items produced by the parser.
/// - `start_and_tag`: bits 0-2 = tag, bits 3-30 = span start (28 bits, max 256 MiB)
/// - `end_and_flag`: bits 0-2 = flag, bits 3-30 = span end (28 bits), bit 31 = 0
///
/// **Format hints variant** (bit 31 = 1): items constructed programmatically.
/// - `start_and_tag`: bits 0-2 = tag, bits 3-31 = projected index (all 1's = not projected)
/// - `end_and_flag`: bit 31 = 1, bits 0-2 = flag, bits 3-30 = format hint bits
#[derive(Copy, Clone)]
#[repr(C)]
pub struct ItemMetadata {
    start_and_tag: u32,
    end_and_flag: u32,
}

impl ItemMetadata {
    /// Creates metadata in span mode (parser-produced items).
    #[inline]
    pub(crate) fn spanned(tag: u32, flag: u32, start: u32, end: u32) -> Self {
        Self {
            start_and_tag: (start << TAG_SHIFT) | tag,
            end_and_flag: (end << FLAG_SHIFT) | flag,
        }
    }

    /// Creates metadata in format-hints mode (programmatically constructed items).
    #[inline]
    pub(crate) fn hints(tag: u32, flag: u32) -> Self {
        Self {
            start_and_tag: NOT_PROJECTED | tag,
            end_and_flag: HINTS_BIT | flag,
        }
    }

    #[inline]
    pub(crate) fn tag(&self) -> u32 {
        self.start_and_tag & TAG_MASK
    }

    #[inline]
    pub(crate) fn flag(&self) -> u32 {
        self.end_and_flag & FLAG_MASK
    }

    #[inline]
    fn set_flag(&mut self, flag: u32) {
        self.end_and_flag = (self.end_and_flag & !FLAG_MASK) | flag;
    }

    #[inline]
    fn set_auto_style(&mut self) {
        self.end_and_flag |= AUTO_STYLE_BIT;
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn is_auto_style(&self) -> bool {
        self.end_and_flag & (HINTS_BIT | AUTO_STYLE_BIT) == (HINTS_BIT | AUTO_STYLE_BIT)
    }

    #[inline]
    fn clear_auto_style(&mut self) {
        self.end_and_flag &= !AUTO_STYLE_BIT;
    }

    /// Returns `true` if this metadata carries a source span (parser-produced).
    #[inline]
    pub(crate) fn is_span_mode(&self) -> bool {
        (self.end_and_flag as i32) >= 0
    }

    /// Returns the source span, or `0..0` if in format-hints mode.
    #[inline]
    pub fn span(&self) -> Span {
        if (self.end_and_flag as i32) >= 0 {
            self.span_unchecked()
        } else {
            Span { start: 0, end: 0 }
        }
    }

    /// Returns the source span without checking the variant.
    /// Valid only during deserialization on parser-produced items.
    /// In span mode, bit 31 is always 0, so all bits above FLAG_SHIFT are span data.
    #[inline]
    pub(crate) fn span_unchecked(&self) -> Span {
        debug_assert!(self.is_span_mode());
        Span::new(
            self.start_and_tag >> TAG_SHIFT,
            self.end_and_flag >> FLAG_SHIFT,
        )
    }

    #[inline]
    pub(crate) fn span_start(&self) -> u32 {
        debug_assert!(self.is_span_mode());
        self.start_and_tag >> TAG_SHIFT
    }

    #[inline]
    fn set_span_start(&mut self, v: u32) {
        debug_assert!(self.is_span_mode());
        self.start_and_tag = (v << TAG_SHIFT) | (self.start_and_tag & TAG_MASK);
    }

    #[inline]
    fn set_span_end(&mut self, v: u32) {
        debug_assert!(self.is_span_mode());
        self.end_and_flag = (v << FLAG_SHIFT) | (self.end_and_flag & FLAG_MASK);
    }

    #[inline]
    fn extend_span_end(&mut self, new_end: u32) {
        debug_assert!(self.is_span_mode());
        let old = self.end_and_flag;
        let current = old >> FLAG_SHIFT;
        self.end_and_flag = (current.max(new_end) << FLAG_SHIFT) | (old & FLAG_MASK);
    }
}

/// Bit 30 of `end_and_flag`: marks a full match during reprojection.
pub(crate) const FULL_MATCH_BIT: u32 = 1 << 30;
/// Bit 29 of `end_and_flag`: when set in format-hints mode, disables
/// source-position reordering for this table's immediate entries.
pub(crate) const IGNORE_SOURCE_ORDER_BIT: u32 = 1 << 29;
/// Bit 28 of `end_and_flag`: when set in format-hints mode, disables
/// copying structural styles from source during reprojection.
pub(crate) const IGNORE_SOURCE_STYLE_BIT: u32 = 1 << 28;
/// Bit 27 of `end_and_flag`: marks an array element as reordered during
/// reprojection. Prevents the emitter from sorting this element back to
/// its original source position in the parent array, without affecting
/// source-ordering of the element's own children.
pub(crate) const ARRAY_REORDERED_BIT: u32 = 1 << 27;
/// Bit 25 of `end_and_flag`: when set in format-hints mode, indicates that
/// an inline array should be emitted in multiline format with one element
/// per line and trailing commas.
pub(crate) const EXPANDED_BIT: u32 = 1 << 25;

impl ItemMetadata {
    /// Returns the projected index (bits 3-31 of `start_and_tag`).
    #[inline]
    pub(crate) fn projected_index(&self) -> u32 {
        self.start_and_tag >> TAG_SHIFT
    }

    /// Branchless mask: span mode -> FLAG_MASK (clears stale span data),
    /// hints mode -> 0xFFFFFFFF (preserves existing hint bits).
    #[inline]
    fn hints_preserve_mask(&self) -> u32 {
        ((self.end_and_flag as i32) >> 31) as u32 | FLAG_MASK
    }

    /// Stores a reprojected index, preserving user-set hint bits when
    /// already in hints mode. Returns `false` if the index doesn't fit.
    #[inline]
    fn set_reprojected_index(&mut self, index: usize) -> bool {
        if index <= (u32::MAX >> TAG_SHIFT) as usize {
            self.start_and_tag = (self.start_and_tag & TAG_MASK) | ((index as u32) << TAG_SHIFT);
            self.end_and_flag = (self.end_and_flag & self.hints_preserve_mask()) | HINTS_BIT;
            true
        } else {
            false
        }
    }

    /// Marks as not projected, preserving user-set hint bits when already
    /// in hints mode and clearing full-match.
    #[inline]
    fn set_reprojected_to_none(&mut self) {
        self.start_and_tag |= NOT_PROJECTED;
        self.end_and_flag =
            (self.end_and_flag & (self.hints_preserve_mask() & !FULL_MATCH_BIT)) | HINTS_BIT;
    }

    #[inline]
    fn set_reprojected_full_match(&mut self) {
        self.end_and_flag |= FULL_MATCH_BIT;
    }

    #[inline]
    pub(crate) fn is_reprojected_full_match(&self) -> bool {
        self.end_and_flag & FULL_MATCH_BIT != 0
    }

    /// Disables source-position reordering for this table's entries.
    #[inline]
    fn set_ignore_source_order(&mut self) {
        self.end_and_flag |= HINTS_BIT | IGNORE_SOURCE_ORDER_BIT;
    }

    /// Returns `true` if source-position reordering is disabled.
    /// Gates on `HINTS_BIT` so stale span-end bits cannot false-positive.
    #[inline]
    pub(crate) fn ignore_source_order(&self) -> bool {
        self.end_and_flag & (HINTS_BIT | IGNORE_SOURCE_ORDER_BIT)
            == (HINTS_BIT | IGNORE_SOURCE_ORDER_BIT)
    }

    /// Marks an array element as reordered during reprojection.
    #[inline]
    fn set_array_reordered(&mut self) {
        self.end_and_flag |= HINTS_BIT | ARRAY_REORDERED_BIT;
    }

    /// Returns `true` if this element was reordered during array reprojection.
    #[inline]
    pub(crate) fn array_reordered(&self) -> bool {
        self.end_and_flag & (HINTS_BIT | ARRAY_REORDERED_BIT) == (HINTS_BIT | ARRAY_REORDERED_BIT)
    }

    /// Disables copying structural styles from source during reprojection.
    #[inline]
    fn set_ignore_source_style(&mut self) {
        self.end_and_flag |= HINTS_BIT | IGNORE_SOURCE_STYLE_BIT;
    }

    /// Returns `true` if source-style copying is disabled for this table.
    #[inline]
    pub(crate) fn ignore_source_style(&self) -> bool {
        self.end_and_flag & (HINTS_BIT | IGNORE_SOURCE_STYLE_BIT)
            == (HINTS_BIT | IGNORE_SOURCE_STYLE_BIT)
    }

    #[inline]
    fn set_expanded(&mut self) {
        self.end_and_flag |= HINTS_BIT | EXPANDED_BIT;
    }

    #[inline]
    pub(crate) fn is_expanded(&self) -> bool {
        self.end_and_flag & (HINTS_BIT | EXPANDED_BIT) == (HINTS_BIT | EXPANDED_BIT)
    }

    #[inline]
    fn clear_expanded(&mut self) {
        self.end_and_flag &= !EXPANDED_BIT;
    }
}

/// Mutable access to an item's metadata that cannot change its tag.
///
/// Handing out `&mut ItemMetadata` would let safe code swap the metadata
/// of two items of different kinds, breaking the tag/payload invariant.
pub(crate) struct MetaMut<'a>(&'a mut ItemMetadata);

impl MetaMut<'_> {
    #[inline]
    pub(crate) fn set_flag(&mut self, flag: u32) {
        self.0.set_flag(flag)
    }

    #[inline]
    pub(crate) fn clear_auto_style(&mut self) {
        self.0.clear_auto_style()
    }

    #[inline]
    pub(crate) fn set_span_start(&mut self, v: u32) {
        self.0.set_span_start(v)
    }

    #[inline]
    pub(crate) fn set_span_end(&mut self, v: u32) {
        self.0.set_span_end(v)
    }

    #[inline]
    pub(crate) fn extend_span_end(&mut self, new_end: u32) {
        self.0.extend_span_end(new_end)
    }

    #[inline]
    pub(crate) fn set_reprojected_index(&mut self, index: usize) -> bool {
        self.0.set_reprojected_index(index)
    }

    #[inline]
    pub(crate) fn set_reprojected_to_none(&mut self) {
        self.0.set_reprojected_to_none()
    }

    #[inline]
    pub(crate) fn set_reprojected_full_match(&mut self) {
        self.0.set_reprojected_full_match()
    }

    #[inline]
    pub(crate) fn set_ignore_source_order(&mut self) {
        self.0.set_ignore_source_order()
    }

    #[inline]
    pub(crate) fn set_array_reordered(&mut self) {
        self.0.set_array_reordered()
    }

    #[inline]
    pub(crate) fn set_ignore_source_style(&mut self) {
        self.0.set_ignore_source_style()
    }

    #[inline]
    pub(crate) fn set_expanded(&mut self) {
        self.0.set_expanded()
    }

    #[inline]
    pub(crate) fn clear_expanded(&mut self) {
        self.0.clear_expanded()
    }
}

/// A handle on the span-end word of an array-of-tables item, kept while
/// the parser fills its last entry.
pub(crate) struct EndSpan<'a>(&'a mut u32);

impl EndSpan<'_> {
    /// Extends the span end to at least `new_end`, keeping the flag bits.
    #[inline]
    pub(crate) fn extend(&mut self, new_end: u32) {
        let old = *self.0;
        let current = old >> FLAG_SHIFT;
        *self.0 = (current.max(new_end) << FLAG_SHIFT) | (old & FLAG_MASK);
    }
}

#[repr(C, align(8))]
union Payload<'de> {
    string: &'de str,
    integer: Integer,
    float: f64,
    boolean: bool,
    array: ManuallyDrop<InternalArray<'de>>,
    table: ManuallyDrop<InnerTable<'de>>,
    datetime: DateTime,
}

/// A parsed TOML value with span information.
///
/// Extract values with the `as_*` methods ([`as_str`](Self::as_str),
/// [`as_i64`](Self::as_i64), [`as_table`](Self::as_table), etc.) or
/// pattern match via [`value`](Self::value) and [`value_mut`](Self::value_mut).
///
/// Items support indexing with `&str` (table lookup) and `usize` (array
/// access). These operators return [`MaybeItem`] and never panic. Missing
/// keys or out-of-bounds indices produce a `None` variant instead.
///
/// # Lookup performance
///
/// String-key lookups (`item["key"]`, [`as_table`](Self::as_table) +
/// [`Table::get`]) perform a linear scan over the table entries, O(n) in
/// the number of keys. For small tables or a handful of lookups, as is
/// typical in TOML, this is fast enough.
///
/// For structured conversion of larger tables, use
/// [`TableHelper`](crate::de::TableHelper) with the [`Context`](crate::de::Context)
/// from [`parse`](crate::parse), which internally uses an index for O(1) lookups.
///
/// # Examples
///
/// ```
/// let arena = rs_toml::Arena::new();
/// let table = rs_toml::parse("x = 42", &arena).unwrap();
/// assert_eq!(table["x"].as_i64(), Some(42));
/// assert_eq!(table["missing"].as_i64(), None);
/// ```
#[repr(C)]
pub struct Item<'de> {
    payload: Payload<'de>,
    meta: ItemMetadata,
}

const _: () = assert!(std::mem::size_of::<Item<'_>>() == 24);
const _: () = assert!(std::mem::align_of::<Item<'_>>() == 8);

impl<'de> Item<'de> {
    /// The packed tag, flag, span and format-hint metadata.
    #[inline]
    pub(crate) fn meta(&self) -> &ItemMetadata {
        &self.meta
    }

    /// Mutable metadata access that preserves the tag.
    #[inline]
    pub(crate) fn meta_mut(&mut self) -> MetaMut<'_> {
        MetaMut(&mut self.meta)
    }
}

impl<'de> From<i64> for Item<'de> {
    fn from(value: i64) -> Self {
        Self::raw_hints(
            TAG_INTEGER,
            FLAG_NONE,
            Payload {
                integer: Integer::from(value),
            },
        )
    }
}
impl<'de> From<i128> for Item<'de> {
    fn from(value: i128) -> Self {
        Self::raw_hints(
            TAG_INTEGER,
            FLAG_NONE,
            Payload {
                integer: Integer::from(value),
            },
        )
    }
}
impl<'de> From<i32> for Item<'de> {
    fn from(value: i32) -> Self {
        Self::from(value as i64)
    }
}
impl<'de> From<&'de str> for Item<'de> {
    fn from(value: &'de str) -> Self {
        Self::raw_hints(TAG_STRING, FLAG_NONE, Payload { string: value })
    }
}

impl<'de> From<f64> for Item<'de> {
    fn from(value: f64) -> Self {
        Self::raw_hints(TAG_FLOAT, FLAG_NONE, Payload { float: value })
    }
}

impl<'de> From<bool> for Item<'de> {
    fn from(value: bool) -> Self {
        Self::raw_hints(TAG_BOOLEAN, FLAG_NONE, Payload { boolean: value })
    }
}

impl<'de> From<DateTime> for Item<'de> {
    fn from(value: DateTime) -> Self {
        Self::raw_hints(TAG_DATETIME, FLAG_NONE, Payload { datetime: value })
    }
}

impl<'de> Item<'de> {
    #[inline]
    fn raw(tag: u32, flag: u32, start: u32, end: u32, payload: Payload<'de>) -> Self {
        Self {
            meta: ItemMetadata::spanned(tag, flag, start, end),
            payload,
        }
    }

    #[inline]
    fn raw_hints(tag: u32, flag: u32, payload: Payload<'de>) -> Self {
        Self {
            meta: ItemMetadata::hints(tag, flag),
            payload,
        }
    }

    /// Creates a string [`Item`] in format-hints mode (no source span).
    #[inline]
    pub fn string(s: &'de str) -> Self {
        Self::raw_hints(TAG_STRING, FLAG_NONE, Payload { string: s })
    }

    #[inline]
    pub(crate) fn string_spanned(s: &'de str, span: Span) -> Self {
        Self::raw(
            TAG_STRING,
            FLAG_NONE,
            span.start,
            span.end,
            Payload { string: s },
        )
    }

    #[inline]
    pub(crate) fn integer_spanned(i: i128, span: Span) -> Self {
        Self::raw(
            TAG_INTEGER,
            FLAG_NONE,
            span.start,
            span.end,
            Payload {
                integer: Integer::from(i),
            },
        )
    }

    #[inline]
    pub(crate) fn float_spanned(f: f64, span: Span) -> Self {
        Self::raw(
            TAG_FLOAT,
            FLAG_NONE,
            span.start,
            span.end,
            Payload { float: f },
        )
    }

    #[inline]
    pub(crate) fn boolean(b: bool, span: Span) -> Self {
        Self::raw(
            TAG_BOOLEAN,
            FLAG_NONE,
            span.start,
            span.end,
            Payload { boolean: b },
        )
    }

    #[inline]
    pub(crate) fn array(a: InternalArray<'de>, span: Span) -> Self {
        Self::raw(
            TAG_ARRAY,
            FLAG_ARRAY,
            span.start,
            span.end,
            Payload {
                array: ManuallyDrop::new(a),
            },
        )
    }

    #[inline]
    pub(crate) fn table(t: InnerTable<'de>, span: Span) -> Self {
        Self::raw(
            TAG_TABLE,
            FLAG_TABLE,
            span.start,
            span.end,
            Payload {
                table: ManuallyDrop::new(t),
            },
        )
    }

    /// Creates an array-of-tables value.
    #[inline]
    pub(crate) fn array_aot(a: InternalArray<'de>, span: Span) -> Self {
        Self::raw(
            TAG_ARRAY,
            FLAG_AOT,
            span.start,
            span.end,
            Payload {
                array: ManuallyDrop::new(a),
            },
        )
    }

    /// Creates a frozen (inline) table value.
    #[inline]
    pub(crate) fn table_frozen(t: InnerTable<'de>, span: Span) -> Self {
        Self::raw(
            TAG_TABLE,
            FLAG_FROZEN,
            span.start,
            span.end,
            Payload {
                table: ManuallyDrop::new(t),
            },
        )
    }

    /// Creates a table with HEADER state (explicitly opened by `[header]`).
    #[inline]
    pub(crate) fn table_header(t: InnerTable<'de>, span: Span) -> Self {
        Self::raw(
            TAG_TABLE,
            FLAG_HEADER,
            span.start,
            span.end,
            Payload {
                table: ManuallyDrop::new(t),
            },
        )
    }

    /// Creates a table with DOTTED state (created by dotted-key navigation).
    #[inline]
    pub(crate) fn table_dotted(t: InnerTable<'de>, span: Span) -> Self {
        Self::raw(
            TAG_TABLE,
            FLAG_DOTTED,
            span.start,
            span.end,
            Payload {
                table: ManuallyDrop::new(t),
            },
        )
    }

    #[inline]
    pub(crate) fn moment(m: DateTime, span: Span) -> Self {
        Self::raw(
            TAG_DATETIME,
            FLAG_NONE,
            span.start,
            span.end,
            Payload { datetime: m },
        )
    }
}

impl<'de> Item<'de> {
    /// Returns the type discriminant of this value.
    #[inline]
    pub fn kind(&self) -> Kind {
        match self.meta.tag() {
            TAG_STRING => Kind::String,
            TAG_INTEGER => Kind::Integer,
            TAG_FLOAT => Kind::Float,
            TAG_BOOLEAN => Kind::Boolean,
            TAG_DATETIME => Kind::DateTime,
            TAG_TABLE => Kind::Table,
            // Constructors only use TAG_STRING..=TAG_ARRAY; TAG_NONE exists
            // only in MaybeItem, which never exposes an `Item` for it.
            _ => Kind::Array,
        }
    }
    #[inline]
    pub(crate) fn tag(&self) -> u32 {
        self.meta.tag()
    }

    /// Returns `true` for leaf values (string, integer, float, boolean,
    /// datetime) that contain no arena-allocated children.
    #[inline]
    pub(crate) fn is_scalar(&self) -> bool {
        self.tag() < TAG_TABLE
    }

    /// Returns the raw 3-bit flag encoding the container sub-kind.
    ///
    /// Prefer [`Table::style`] or [`Array::style`] for a typed alternative.
    #[inline]
    pub fn flag(&self) -> u32 {
        self.meta.flag()
    }

    /// Returns the byte-offset span of this value in the source document.
    /// Only valid on parser-produced items (span mode).
    #[inline]
    pub(crate) fn span_unchecked(&self) -> Span {
        self.meta.span_unchecked()
    }

    /// Returns the source span, or `0..0` if this item was constructed
    /// programmatically (format-hints mode).
    #[inline]
    pub fn span(&self) -> Span {
        self.meta.span()
    }

    /// Returns the TOML type name (e.g. `"string"`, `"integer"`, `"table"`).
    #[inline]
    pub fn type_str(&self) -> &'static &'static str {
        match self.kind() {
            Kind::String => &"string",
            Kind::Integer => &"integer",
            Kind::Float => &"float",
            Kind::Boolean => &"boolean",
            Kind::Array => &"array",
            Kind::Table => &"table",
            Kind::DateTime => &"datetime",
        }
    }

    #[inline]
    pub(crate) fn is_table(&self) -> bool {
        self.flag() >= FLAG_TABLE
    }

    #[inline]
    pub(crate) fn is_frozen(&self) -> bool {
        self.flag() == FLAG_FROZEN
    }

    #[inline]
    pub(crate) fn is_aot(&self) -> bool {
        self.flag() == FLAG_AOT
    }

    #[inline]
    pub(crate) fn has_header_bit(&self) -> bool {
        self.flag() == FLAG_HEADER
    }

    #[inline]
    pub(crate) fn has_dotted_bit(&self) -> bool {
        self.flag() == FLAG_DOTTED
    }

    /// Returns `true` if this is an implicit intermediate table, a plain
    /// table that is neither a `[header]` section, a dotted-key intermediate,
    /// nor a frozen inline `{ }` table. These entries act as structural
    /// parents for header sections and have no text of their own.
    #[inline]
    pub(crate) fn is_implicit_table(&self) -> bool {
        self.flag() == FLAG_TABLE
    }

    /// Splits an array item into a handle on its span end and its storage,
    /// or returns `None` if this is not an array.
    #[inline]
    pub(crate) fn split_array_end_flag(
        &mut self,
    ) -> Option<(EndSpan<'_>, &mut InternalArray<'de>)> {
        if self.meta.tag() != TAG_ARRAY {
            return None;
        }
        // SAFETY: the tag is TAG_ARRAY, so `payload.array` is the active
        // union field. `payload` and `meta.end_and_flag` are disjoint fields,
        // so the two mutable borrows do not alias. `EndSpan` can only change
        // the span-end/flag word, never the tag.
        let array: &mut InternalArray<'de> = unsafe { &mut self.payload.array };
        Some((EndSpan(&mut self.meta.end_and_flag), array))
    }
}

impl<'de> Item<'de> {
    /// Returns a borrowed view for pattern matching.
    pub fn value(&self) -> Value<'_, 'de> {
        // SAFETY: kind() returns the discriminant set at construction. Each
        // match arm reads the union field that was written for that discriminant.
        unsafe {
            match self.kind() {
                Kind::String => Value::String(&self.payload.string),
                Kind::Integer => Value::Integer(&self.payload.integer),
                Kind::Float => Value::Float(&self.payload.float),
                Kind::Boolean => Value::Boolean(&self.payload.boolean),
                Kind::Array => Value::Array(self.as_array_unchecked()),
                Kind::Table => Value::Table(self.as_table_unchecked()),
                Kind::DateTime => Value::DateTime(&self.payload.datetime),
            }
        }
    }

    /// Returns a mutable view for pattern matching.
    pub fn value_mut(&mut self) -> ValueMut<'_, 'de> {
        // SAFETY: kind() returns the discriminant set at construction. Each
        // match arm accesses the union field that was written for that discriminant.
        unsafe {
            match self.kind() {
                Kind::String => ValueMut::String(&mut self.payload.string),
                Kind::Integer => ValueMut::Integer(&mut self.payload.integer),
                Kind::Float => ValueMut::Float(&mut self.payload.float),
                Kind::Boolean => ValueMut::Boolean(&mut self.payload.boolean),
                Kind::Array => ValueMut::Array(self.as_array_mut_unchecked()),
                Kind::Table => ValueMut::Table(self.as_table_mut_unchecked()),
                Kind::DateTime => ValueMut::DateTime(&self.payload.datetime),
            }
        }
    }
}
impl<'de> Item<'de> {
    /// Returns a borrowed string if this is a string value.
    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        if self.tag() == TAG_STRING {
            // SAFETY: tag check guarantees the payload is a string.
            Some(unsafe { self.payload.string })
        } else {
            None
        }
    }

    #[doc(hidden)]
    /// Used in derive macro for style attributes
    pub fn with_style_of_array_or_table(mut self, style: TableStyle) -> Item<'de> {
        match self.value_mut() {
            ValueMut::Table(table) => table.set_style(style),
            ValueMut::Array(array) => match style {
                TableStyle::Header => array.set_style(ArrayStyle::Header),
                TableStyle::Inline => array.set_style(ArrayStyle::Inline),
                _ => (),
            },
            _ => (),
        }
        self
    }

    /// Returns an `i128` if this is an integer value.
    #[inline]
    pub fn as_i128(&self) -> Option<i128> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            Some(unsafe { self.payload.integer.as_i128() })
        } else {
            None
        }
    }

    /// Returns an `i64` if this is an integer value that fits in the `i64` range.
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            unsafe { self.payload.integer.as_i64() }
        } else {
            None
        }
    }

    /// Returns a `u64` if this is an integer value that fits in the `u64` range.
    #[inline]
    pub fn as_u64(&self) -> Option<u64> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            unsafe { self.payload.integer.as_u64() }
        } else {
            None
        }
    }

    /// Returns an `f64` if this is a float or integer value.
    ///
    /// Integer values are converted to `f64` via `as` cast (lossy for large
    /// values outside the 2^53 exact-integer range).
    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        match self.value() {
            Value::Float(f) => Some(*f),
            Value::Integer(i) => Some(i.as_i128() as f64),
            _ => None,
        }
    }

    /// Returns a `bool` if this is a boolean value.
    #[inline]
    pub fn as_bool(&self) -> Option<bool> {
        if self.tag() == TAG_BOOLEAN {
            // SAFETY: tag check guarantees the payload is a boolean.
            Some(unsafe { self.payload.boolean })
        } else {
            None
        }
    }

    /// Returns a borrowed array if this is an array value.
    #[inline]
    pub fn as_array(&self) -> Option<&Array<'de>> {
        if self.tag() == TAG_ARRAY {
            // SAFETY: tag check guarantees this item is an array variant.
            Some(unsafe { self.as_array_unchecked() })
        } else {
            None
        }
    }

    /// Returns a borrowed table if this is a table value.
    #[inline]
    pub fn as_table(&self) -> Option<&Table<'de>> {
        if self.tag() == TAG_TABLE {
            // SAFETY: is_table() check guarantees this item is a table variant.
            Some(unsafe { self.as_table_unchecked() })
        } else {
            None
        }
    }

    /// Returns a borrowed [`DateTime`] if this is a datetime value.
    #[inline]
    pub fn as_datetime(&self) -> Option<&DateTime> {
        if self.tag() == TAG_DATETIME {
            // SAFETY: tag check guarantees the payload is a moment.
            Some(unsafe { &self.payload.datetime })
        } else {
            None
        }
    }

    /// Returns a mutable array reference.
    #[inline]
    pub fn as_array_mut(&mut self) -> Option<&mut Array<'de>> {
        if self.tag() == TAG_ARRAY {
            // SAFETY: tag check guarantees this item is an array variant.
            Some(unsafe { self.as_array_mut_unchecked() })
        } else {
            None
        }
    }

    /// Consumes this item, returning the table if it is one.
    #[inline]
    pub fn into_table(self) -> Option<Table<'de>> {
        if self.tag() == TAG_TABLE {
            // SAFETY: is_table() guarantees the active union field is `table`.
            // Item and Table have identical size, alignment, and repr(C) layout
            // (verified by const assertions on Table). Item has no Drop impl.
            Some(unsafe { std::mem::transmute::<Item<'de>, Table<'de>>(self) })
        } else {
            None
        }
    }

    /// Returns a mutable table reference.
    #[inline]
    pub fn as_table_mut(&mut self) -> Option<&mut Table<'de>> {
        if self.tag() == TAG_TABLE {
            // SAFETY: is_table() check guarantees this item is a table variant.
            Some(unsafe { self.as_table_mut_unchecked() })
        } else {
            None
        }
    }

    /// Reinterprets this [`Item`] as an [`Array`] (shared reference).
    ///
    /// # Safety
    ///
    /// - `self.tag()` must be `TAG_ARRAY`.
    #[inline]
    unsafe fn as_array_unchecked(&self) -> &Array<'de> {
        debug_assert!(self.tag() == TAG_ARRAY);
        // SAFETY: Item is #[repr(C)] { payload: Payload, meta: ItemMetadata }.
        // Array is #[repr(C)] { value: InternalArray, meta: ItemMetadata }.
        // Payload is a union whose `array` field is ManuallyDrop<InternalArray>
        // (#[repr(transparent)]). Both types are 24 bytes, align 8 (verified by
        // const assertions). Field offsets match: data at 0..16, metadata at 16..24.
        // Caller guarantees the active union field is `array`.
        unsafe { &*(self as *const Item<'de>).cast::<Array<'de>>() }
    }

    /// Reinterprets this [`Item`] as an [`Array`] (mutable reference).
    ///
    /// # Safety
    ///
    /// - `self.tag()` must be `TAG_ARRAY`.
    #[inline]
    unsafe fn as_array_mut_unchecked(&mut self) -> &mut Array<'de> {
        debug_assert!(self.tag() == TAG_ARRAY);
        // SAFETY: Same layout argument as as_array_unchecked.
        unsafe { &mut *(self as *mut Item<'de>).cast::<Array<'de>>() }
    }

    /// Reinterprets this [`Item`] as a [`Table`] (mutable reference).
    ///
    /// # Safety
    ///
    /// - `self.is_table()` must be true.
    #[inline]
    unsafe fn as_table_mut_unchecked(&mut self) -> &mut Table<'de> {
        debug_assert!(self.tag() == TAG_TABLE);
        // SAFETY: Item is #[repr(C)] { payload: Payload, meta: ItemMetadata }.
        // Table is #[repr(C)] { value: InnerTable, meta: ItemMetadata }.
        // Payload's `table` field is ManuallyDrop<InnerTable> (#[repr(transparent)]).
        // Both are 24 bytes, align 8 (const assertions). Field offsets match.
        // Caller guarantees the active union field is `table`.
        unsafe { &mut *(self as *mut Item<'de>).cast::<Table<'de>>() }
    }

    /// Reinterprets this [`Item`] as a [`Table`] (shared reference).
    ///
    /// # Safety
    ///
    /// - `self.is_table()` must be true.
    #[inline]
    unsafe fn as_table_unchecked(&self) -> &Table<'de> {
        debug_assert!(self.tag() == TAG_TABLE);
        // SAFETY: Same layout argument as as_table_mut_unchecked.
        unsafe { &*(self as *const Item<'de>).cast::<Table<'de>>() }
    }

    /// Returns `true` if the value is a non-empty table.
    #[inline]
    pub fn has_keys(&self) -> bool {
        self.as_table().is_some_and(|t| !t.is_empty())
    }

    /// Returns `true` if the value is a table containing `key`.
    #[inline]
    pub fn has_key(&self, key: &str) -> bool {
        self.as_table().is_some_and(|t| t.contains_key(key))
    }

    /// Deep-clones this item into `arena`.
    ///
    /// Scalar values are copied directly. Tables and arrays are recursively
    /// cloned with new arena-allocated storage. String keys and values
    /// continue to reference their original memory.
    pub fn clone_in(&self, arena: &'de Arena) -> Item<'de> {
        if self.is_scalar() {
            // SAFETY: Scalar items have tags 0..=4 (STRING, INTEGER, FLOAT,
            // BOOLEAN, DATETIME). None of these own arena-allocated children.
            // STRING contains a &'de str (shared reference to input/arena data)
            // which is safe to duplicate. Item has no Drop impl, so ptr::read
            // is a plain bitwise copy with no double-free risk.
            unsafe { std::ptr::read(self) }
        } else if self.tag() == TAG_ARRAY {
            // SAFETY: tag == TAG_ARRAY guarantees payload.array is the active
            // union field.
            let cloned = unsafe { self.payload.array.clone_in(arena) };
            Item {
                payload: Payload {
                    array: ManuallyDrop::new(cloned),
                },
                meta: self.meta,
            }
        } else {
            // SAFETY: Tags are 0..=6. is_scalar() is false (tag >= 5) and
            // tag != TAG_ARRAY (6), so tag must be TAG_TABLE (5). Therefore
            // payload.table is the active union field.
            let cloned = unsafe { self.payload.table.clone_in(arena) };
            Item {
                payload: Payload {
                    table: ManuallyDrop::new(cloned),
                },
                meta: self.meta,
            }
        }
    }
}

/// A nullable reference to a parsed TOML value.
///
/// `MaybeItem` is returned by the index operators (`[]`) on [`Item`],
/// [`Table`], [`Array`], and `MaybeItem` itself. It acts like an
/// [`Option<&Item>`] that can be further indexed without panicking. Chained
/// lookups on missing keys simply propagate the `None` state.
///
/// Use the `as_*` accessors to extract a value, or call [`item`](Self::item)
/// to get back an `Option<&Item>`.
///
/// # Examples
///
/// ```
/// use rs_toml::Arena;
///
/// let arena = Arena::new();
/// let table = rs_toml::parse(r#"
/// [server]
/// host = "localhost"
/// port = 8080
/// "#, &arena).unwrap();
///
/// // Successful nested lookup.
/// assert_eq!(table["server"]["host"].as_str(), Some("localhost"));
/// assert_eq!(table["server"]["port"].as_i64(), Some(8080));
///
/// // Missing keys propagate through chained indexing without panicking.
/// assert_eq!(table["server"]["missing"].as_str(), None);
/// assert_eq!(table["nonexistent"]["deep"]["path"].as_str(), None);
///
/// // Convert back to an Option<&Item> when needed.
/// assert!(table["server"]["host"].item().is_some());
/// assert!(table["nope"].item().is_none());
/// ```
#[repr(C)]
pub struct MaybeItem<'de> {
    payload: Payload<'de>,
    meta: ItemMetadata,
}

// SAFETY: MaybeItem is only constructed as either (a) the static NONE sentinel
// (payload is zeroed, tag is TAG_NONE — no pointers are dereferenced), or
// (b) a reinterpretation of &Item via from_ref. In both cases the data is
// accessed only through shared references. All payload variants (integers,
// floats, bools, &str, Array, InnerTable) are safe to share across threads
// when behind a shared reference.
unsafe impl Sync for MaybeItem<'_> {}

pub(crate) static NONE: MaybeItem<'static> = MaybeItem {
    payload: Payload {
        integer: Integer::ZERO,
    },
    meta: ItemMetadata {
        start_and_tag: TAG_NONE,
        end_and_flag: FLAG_NONE,
    },
};

impl<'de> MaybeItem<'de> {
    /// Views an [`Item`] reference as a `MaybeItem`.
    pub fn from_ref<'a>(item: &'a Item<'de>) -> &'a Self {
        // SAFETY: Item and MaybeItem are both #[repr(C)] with identical field
        // layout (Payload, ItemMetadata). Size and alignment equality is verified
        // by const assertions.
        unsafe { &*(item as *const Item<'de>).cast::<MaybeItem<'de>>() }
    }
    #[inline]
    pub(crate) fn tag(&self) -> u32 {
        self.meta.tag()
    }
    /// Returns the underlying [`Item`], or [`None`] if this is a missing value.
    pub fn item(&self) -> Option<&Item<'de>> {
        if self.tag() != TAG_NONE {
            // SAFETY: tag != TAG_NONE means this was created via from_ref from
            // a valid Item. Item and MaybeItem have identical repr(C) layout.
            Some(unsafe { &*(self as *const MaybeItem<'de>).cast::<Item<'de>>() })
        } else {
            None
        }
    }
    /// Returns a borrowed string if this is a string value.
    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        if self.tag() == TAG_STRING {
            // SAFETY: tag check guarantees the payload is a string.
            Some(unsafe { self.payload.string })
        } else {
            None
        }
    }

    /// Returns an `i128` if this is an integer value.
    #[inline]
    pub fn as_i128(&self) -> Option<i128> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            Some(unsafe { self.payload.integer.as_i128() })
        } else {
            None
        }
    }

    /// Returns an `i64` if this is an integer value that fits in the `i64` range.
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            unsafe { self.payload.integer.as_i64() }
        } else {
            None
        }
    }

    /// Returns a `u64` if this is an integer value that fits in the `u64` range.
    #[inline]
    pub fn as_u64(&self) -> Option<u64> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            unsafe { self.payload.integer.as_u64() }
        } else {
            None
        }
    }

    /// Returns an `f64` if this is a float or integer value.
    ///
    /// Integer values are converted to `f64` via `as` cast (lossy for large
    /// values outside the 2^53 exact-integer range).
    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        self.item()?.as_f64()
    }

    /// Returns a `bool` if this is a boolean value.
    #[inline]
    pub fn as_bool(&self) -> Option<bool> {
        if self.tag() == TAG_BOOLEAN {
            // SAFETY: tag check guarantees the payload is a boolean.
            Some(unsafe { self.payload.boolean })
        } else {
            None
        }
    }

    /// Returns a borrowed array if this is an array value.
    #[inline]
    pub fn as_array(&self) -> Option<&Array<'de>> {
        if self.tag() == TAG_ARRAY {
            // SAFETY: tag == TAG_ARRAY guarantees the payload is an array.
            // MaybeItem and Array have identical repr(C) layout (verified by
            // const size/align assertions on Item and Array).
            Some(unsafe { &*(self as *const Self).cast::<Array<'de>>() })
        } else {
            None
        }
    }

    /// Returns a borrowed table if this is a table value.
    #[inline]
    pub fn as_table(&self) -> Option<&Table<'de>> {
        if self.tag() == TAG_TABLE {
            // SAFETY: tag == TAG_TABLE guarantees the payload is a table.
            // MaybeItem and Table have identical repr(C) layout (verified by
            // const size/align assertions on Item and Table).
            Some(unsafe { &*(self as *const Self).cast::<Table<'de>>() })
        } else {
            None
        }
    }

    /// Returns a borrowed [`DateTime`] if this is a datetime value.
    #[inline]
    pub fn as_datetime(&self) -> Option<&DateTime> {
        if self.tag() == TAG_DATETIME {
            // SAFETY: tag check guarantees the payload is a moment.
            Some(unsafe { &self.payload.datetime })
        } else {
            None
        }
    }

    /// Returns the source span, or `0..0` if this is a missing value.
    pub fn span(&self) -> Span {
        if let Some(item) = self.item() {
            item.span_unchecked()
        } else {
            Span::default()
        }
    }

    /// Returns a borrowed [`Value`] for pattern matching, or [`None`] if missing.
    pub fn value(&self) -> Option<Value<'_, 'de>> {
        if let Some(item) = self.item() {
            Some(item.value())
        } else {
            None
        }
    }
}

/// A TOML table, a transient collection of key/value pairs inside an [`Item`].
///
/// `Table` is an intermediate structure for converting between Rust types and
/// TOML through [`FromToml`] and [`ToToml`]. It is the top level value
/// returned by [`parse`](crate::parse) and the value inside any `[section]`
/// or inline `{ ... }` table.
///
/// <div class="warning">
///
/// Entries are stored in insertion order, but the TOML specification defines
/// table keys as unordered. Avoid relying on iteration order for semantic
/// purposes.
///
/// </div>
///
/// # Accessing values
///
/// Use `table["key"]` for index access, which returns a [`MaybeItem`] that
/// never panics on missing keys and supports chained indexing. Use
/// [`get`](Self::get) or [`get_mut`](Self::get_mut) for `Option` based access.
///
/// For structured extraction, use [`Item::table_helper`](crate::Item::table_helper)
/// to create a [`TableHelper`](crate::de::TableHelper).
///
/// # Lookup performance
///
/// Direct lookups ([`get`](Self::get), `table["key"]`) perform a linear scan
/// over entries, O(n) in the number of keys. For small tables or a handful
/// of lookups, as is typical in TOML, this is fast enough.
///
/// For structured conversion of larger tables, use
/// [`TableHelper`](crate::de::TableHelper) via
/// [`Document::table_helper`](crate::Document::table_helper) or
/// [`Item::table_helper`](crate::Item::table_helper), which use the
/// parser's hash index for O(1) lookups.
///
/// # Constructing tables
///
/// To build a table programmatically, create one with [`Table::new`] and
/// insert entries with [`Table::insert`]. An [`Arena`](crate::Arena) is
/// required because entries are arena allocated.
///
/// # Iteration
///
/// `Table` implements [`IntoIterator`] (both by reference and by value),
/// yielding `(`[`Key`]`, `[`Item`]`)` pairs.
///
/// [`FromToml`]: crate::FromToml
/// [`ToToml`]: crate::ToToml
#[repr(C)]
pub struct Table<'de> {
    pub(crate) value: InnerTable<'de>,
    meta: ItemMetadata,
}

const _: () = assert!(std::mem::size_of::<Table<'_>>() == std::mem::size_of::<Item<'_>>());
const _: () = assert!(std::mem::align_of::<Table<'_>>() == std::mem::align_of::<Item<'_>>());

impl<'de> Table<'de> {
    /// Creates an empty table in format-hints mode (no source span).
    ///
    /// The table starts with automatic style: normalization will choose
    /// between inline and header based on content. Call [`set_style`](Self::set_style)
    /// to override.
    pub fn new() -> Table<'de> {
        let mut meta = ItemMetadata::hints(TAG_TABLE, FLAG_TABLE);
        meta.set_auto_style();
        Table {
            meta,
            value: InnerTable::new(),
        }
    }

    /// Creates an empty table with pre-allocated capacity.
    ///
    /// Returns `None` if `cap` exceeds [`MAX_CAPACITY`](crate::MAX_CAPACITY).
    pub fn try_with_capacity(cap: usize, arena: &'de Arena) -> Option<Table<'de>> {
        if cap > crate::MAX_CAPACITY {
            return None;
        }
        let cap: u32 = cap.try_into().ok()?;
        let mut meta = ItemMetadata::hints(TAG_TABLE, FLAG_TABLE);
        meta.set_auto_style();
        Some(Table {
            meta,
            value: InnerTable::with_capacity(cap, arena),
        })
    }

    /// Creates an empty table in span mode (parser-produced).
    pub(crate) fn new_spanned(span: Span) -> Table<'de> {
        Table {
            meta: ItemMetadata::spanned(TAG_TABLE, FLAG_TABLE, span.start, span.end),
            value: InnerTable::new(),
        }
    }

    /// Returns the source span, or `0..0` if this table was constructed
    /// programmatically (format-hints mode).
    pub fn span(&self) -> Span {
        self.meta.span()
    }
}

impl<'de> Table<'de> {
    /// The packed tag, flag, span and format-hint metadata.
    #[inline]
    pub(crate) fn meta(&self) -> &ItemMetadata {
        &self.meta
    }

    /// Mutable metadata access that preserves the tag.
    #[inline]
    pub(crate) fn meta_mut(&mut self) -> MetaMut<'_> {
        MetaMut(&mut self.meta)
    }

    /// Converts this `Table` into an [`Item`] with the same span and payload.
    pub fn as_item(&self) -> &Item<'de> {
        // SAFETY: Table is #[repr(C)] { InnerTable, ItemMetadata }.
        // Item  is #[repr(C)] { Payload,    ItemMetadata }.
        // Payload is a union whose `table` field is ManuallyDrop<InnerTable>
        // (#[repr(transparent)]). Both types are 24 bytes, align 8 (verified
        // by const assertions). The field offsets match (data at 0..16,
        // metadata at 16..24). Only a shared reference is returned, so no
        // mutation can change the tag.
        unsafe { &*(self as *const Table<'de>).cast::<Item<'de>>() }
    }

    /// Converts this `Table` into an [`Item`] with the same span and payload.
    pub fn into_item(self) -> Item<'de> {
        // SAFETY: Same layout argument as as_item(). Size and alignment
        // equality verified by const assertions below. The tag in
        // ItemMetadata is preserved unchanged through the transmute.
        unsafe { std::mem::transmute(self) }
    }

    /// Deep-clones this table into `arena`. Keys and strings are shared
    /// with the source.
    pub fn clone_in(&self, arena: &'de Arena) -> Table<'de> {
        Table {
            value: self.value.clone_in(arena),
            meta: self.meta,
        }
    }
}

/// A TOML array with span information.
///
/// An `Array` stores [`Item`] elements in insertion order with arena-allocated
/// backing storage. It carries the byte-offset [`Span`] from the source
/// document.
///
/// # Accessing elements
///
/// - **Index operator**: `array[i]` returns a [`MaybeItem`] that never
///   panics on out-of-bounds access.
/// - **`get` / `get_mut`**: return `Option<&Item>` / `Option<&mut Item>`.
/// - **Iteration**: `for item in &array { ... }`.
///
/// # Mutation
///
/// [`push`](Self::push) appends an element. An [`Arena`] is required because
/// array storage is arena-allocated.
#[repr(C)]
pub struct Array<'de> {
    pub(crate) value: InternalArray<'de>,
    meta: ItemMetadata,
}

const _: () = assert!(std::mem::size_of::<Array<'_>>() == std::mem::size_of::<Item<'_>>());
const _: () = assert!(std::mem::align_of::<Array<'_>>() == std::mem::align_of::<Item<'_>>());

impl<'de> Array<'de> {
    /// Creates an empty array in format-hints mode (no source span).
    ///
    /// The array starts with automatic style: normalization will choose
    /// between inline and header (array-of-tables) based on content. Call
    /// [`set_style`](Self::set_style) to override.
    pub fn new() -> Self {
        let mut meta = ItemMetadata::hints(TAG_ARRAY, FLAG_ARRAY);
        meta.set_auto_style();
        Self {
            meta,
            value: InternalArray::new(),
        }
    }

    /// Creates an empty array with pre-allocated capacity.
    ///
    /// Returns `None` if `cap` exceeds [`MAX_CAPACITY`](crate::MAX_CAPACITY).
    pub fn try_with_capacity(cap: usize, arena: &'de Arena) -> Option<Self> {
        if cap > crate::MAX_CAPACITY {
            return None;
        }
        let cap: u32 = cap.try_into().ok()?;
        let mut meta = ItemMetadata::hints(TAG_ARRAY, FLAG_ARRAY);
        meta.set_auto_style();
        Some(Self {
            meta,
            value: InternalArray::with_capacity(cap, arena),
        })
    }

    /// Creates an empty array in span mode (parser-produced).
    #[cfg(test)]
    pub(crate) fn new_spanned(span: Span) -> Self {
        Self {
            meta: ItemMetadata::spanned(TAG_ARRAY, FLAG_ARRAY, span.start, span.end),
            value: InternalArray::new(),
        }
    }

    /// Returns the byte-offset span of this array in the source document.
    /// Only valid on parser-produced arrays (span mode).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn span_unchecked(&self) -> Span {
        self.meta.span_unchecked()
    }

    /// Returns the source span, or `0..0` if this array was constructed
    /// programmatically (format-hints mode).
    pub fn span(&self) -> Span {
        self.meta.span()
    }
}

impl<'de> Array<'de> {
    /// The packed tag, flag, span and format-hint metadata.
    #[inline]
    pub(crate) fn meta(&self) -> &ItemMetadata {
        &self.meta
    }

    /// Mutable metadata access that preserves the tag.
    #[inline]
    pub(crate) fn meta_mut(&mut self) -> MetaMut<'_> {
        MetaMut(&mut self.meta)
    }

    /// Converts this `Array` into an [`Item`] with the same span and payload.
    pub fn as_item(&self) -> &Item<'de> {
        // SAFETY: Array is #[repr(C)] { InternalArray, ItemMetadata }.
        // Item  is #[repr(C)] { Payload,       ItemMetadata }.
        // Payload is a union whose `array` field is ManuallyDrop<InternalArray>
        // (#[repr(transparent)]). Both types are 24 bytes, align 8 (verified
        // by const assertions). The field offsets match (data at 0..16,
        // metadata at 16..24). Only a shared reference is returned.
        unsafe { &*(self as *const Array<'de>).cast::<Item<'de>>() }
    }

    /// Converts this `Array` into an [`Item`] with the same span and payload.
    pub fn into_item(self) -> Item<'de> {
        // SAFETY: Same layout argument as as_item(). Size and alignment
        // equality verified by const assertions. The tag in ItemMetadata is
        // preserved unchanged through the transmute.
        unsafe { std::mem::transmute(self) }
    }

    /// Deep-clones this array into `arena`. Keys and strings are shared
    /// with the source.
    pub fn clone_in(&self, arena: &'de Arena) -> Array<'de> {
        Array {
            value: self.value.clone_in(arena),
            meta: self.meta,
        }
    }
}
