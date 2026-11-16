#[cfg(test)]
#[path = "../../tests/unit/item/table.rs"]
mod tests;

use crate::item::{
    FLAG_DOTTED, FLAG_FROZEN, FLAG_HEADER, FLAG_TABLE, Item, Key, MaybeItem, NONE, TableStyle,
};
use crate::parser::KeyRef;
use crate::raw::vec::ArenaVec;

use crate::arena::Arena;

pub(crate) type TableIndex<'de> = foldhash::HashMap<KeyRef<'de>, usize>;

type TableEntry<'de> = (Key<'de>, Item<'de>);

/// A TOML table: a flat list of key-value pairs with linear lookup.
#[repr(transparent)]
#[derive(Default)]
pub(crate) struct InnerTable<'de>(ArenaVec<'de, TableEntry<'de>>);

impl<'de> InnerTable<'de> {
    /// Creates an empty table.
    #[inline]
    pub fn new() -> Self {
        Self(ArenaVec::new())
    }

    pub(crate) fn with_capacity(cap: u32, arena: &'de Arena) -> Self {
        Self(ArenaVec::with_capacity(cap, arena))
    }

    /// Inserts a key-value pair. Does **not** check for duplicates.
    #[inline]
    pub fn insert_unique(
        &mut self,
        key: Key<'de>,
        item: Item<'de>,
        arena: &'de Arena,
    ) -> &mut TableEntry<'de> {
        self.0.push((key, item), arena)
    }

    /// Returns the number of entries.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the table has no entries.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Looks up a key using the parser's hash index when the table is large
    /// enough, falling back to a linear scan for small tables.
    ///
    /// Both the table and the index must originate from the same `parse` call
    /// and neither must have been mutated since parsing.
    pub(crate) fn get_entry_with_index(
        &self,
        key: &str,
        index: &TableIndex<'_>,
    ) -> Option<&TableEntry<'de>> {
        if self.len() > crate::parser::INDEXED_TABLE_THRESHOLD {
            let first_key_span = self.first_key_span_start();
            let i = *index.get(&KeyRef::new(key, first_key_span))?;
            self.entries().get(i)
        } else {
            for entry in self.entries() {
                if entry.0.name == key {
                    return Some(entry);
                }
            }
            None
        }
    }

    pub(crate) fn get_entry_with_maybe_index(
        &self,
        key: &str,
        index: Option<&TableIndex<'_>>,
    ) -> Option<&TableEntry<'de>> {
        if self.len() > crate::parser::INDEXED_TABLE_THRESHOLD {
            if let Some(index) = index {
                let first_key_span = self.first_key_span_start();
                let i = *index.get(&KeyRef::new(key, first_key_span))?;
                return self.entries().get(i);
            }
        }
        for entry in self.entries() {
            if entry.0.name == key {
                return Some(entry);
            }
        }
        None
    }
    /// Linear scan for a key, returning both key and value references.
    pub fn get_entry(&self, name: &str) -> Option<(&Key<'de>, &Item<'de>)> {
        for (key, item) in self.entries() {
            if key.name == name {
                return Some((key, item));
            }
        }
        None
    }

    /// Returns a reference to the value for `name`.
    pub fn get(&self, name: &str) -> Option<&Item<'de>> {
        for (key, item) in self.entries() {
            if key.name == name {
                return Some(item);
            }
        }
        None
    }

    /// Returns a mutable reference to the value for `name`.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Item<'de>> {
        for (key, item) in self.entries_mut() {
            if key.name == name {
                return Some(item);
            }
        }
        None
    }

    /// Returns `true` if the table contains the key.
    #[inline]
    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Removes the first entry matching `name`, returning the key-value pair.
    /// Uses swap-remove, so the ordering of remaining entries may change.
    pub fn remove_entry(&mut self, name: &str) -> Option<(Key<'de>, Item<'de>)> {
        let idx = self.find_index(name)?;
        Some(self.remove_at(idx))
    }

    /// Returns the span start of the first key. Used as a table discriminator
    /// in the parser's hash index.
    #[inline]
    pub(crate) fn first_key_span_start(&self) -> u32 {
        // Callers only ask for non-empty tables; an empty table has no
        // indexed keys, so the fallback value is never looked up.
        self.entries().first().map_or(0, |(key, _)| key.span.start)
    }

    /// Returns a slice of all entries.
    #[inline]
    pub fn entries(&self) -> &[TableEntry<'de>] {
        self.0.as_slice()
    }

    #[inline]
    pub fn entries_mut(&mut self) -> &mut [TableEntry<'de>] {
        self.0.as_mut_slice()
    }

    pub(crate) fn find_index(&self, name: &str) -> Option<usize> {
        for (i, (key, _)) in self.entries().iter().enumerate() {
            if key.name == name {
                return Some(i);
            }
        }
        None
    }

    /// Remove entry at `idx` by swapping it with the last entry.
    fn remove_at(&mut self, idx: usize) -> (Key<'de>, Item<'de>) {
        self.0.swap_remove(idx)
    }

    /// Deep-clones this table into `arena`. Keys and strings are shared
    /// with the source.
    pub(crate) fn clone_in(&self, arena: &'de Arena) -> Self {
        if self.is_empty() {
            return Self::new();
        }
        Self(
            self.0
                .map_in(arena, |(key, item)| (*key, item.clone_in(arena))),
        )
    }
}

impl std::fmt::Debug for InnerTable<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();
        for (k, v) in self.entries() {
            map.entry(k, v);
        }
        map.finish()
    }
}

/// Consuming iterator over a [`Table`], yielding `(`[`Key`]`, `[`Item`]`)` pairs.
pub struct IntoIter<'de>(crate::raw::vec::IntoIter<'de, TableEntry<'de>>);

impl<'de> Iterator for IntoIter<'de> {
    type Item = (Key<'de>, Item<'de>);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl ExactSizeIterator for IntoIter<'_> {}

pub use crate::raw::item::Table;

impl<'de> Default for Table<'de> {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Table<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl<'de> Table<'de> {
    /// Inserts a key-value pair, replacing any existing entry with the same key.
    ///
    /// Performs an O(n) linear scan for duplicates. Prefer [`Table::insert_unique`]
    /// when the key is known to be absent.
    pub fn insert(&mut self, key: Key<'de>, value: Item<'de>, arena: &'de Arena) {
        if let Some(existing) = self.get_mut(key.name) {
            *existing = value;
            return;
        }
        self.value.insert_unique(key, value, arena);
    }

    /// Inserts a key-value pair without checking for duplicates.
    ///
    /// Inserting a duplicate key is sound but the behavior is unspecified. It may
    /// panic, produce invalid TOML on emit, or cause deserialization errors.
    pub fn insert_unique(&mut self, key: Key<'de>, value: Item<'de>, arena: &'de Arena) {
        self.value.insert_unique(key, value, arena);
    }
    /// Returns the number of entries.
    #[inline]
    pub fn len(&self) -> usize {
        self.value.len()
    }

    /// Returns `true` if the table has no entries.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Linear scan for a key, returning both key and value references.
    pub fn get_key_value(&self, name: &str) -> Option<(&Key<'de>, &Item<'de>)> {
        self.value.get_entry(name)
    }

    /// Returns a reference to the value for `name`.
    pub fn get(&self, name: &str) -> Option<&Item<'de>> {
        self.value.get(name)
    }

    /// Returns a mutable reference to the value for `name`.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Item<'de>> {
        self.value.get_mut(name)
    }

    /// Returns `true` if the table contains the key.
    #[inline]
    pub fn contains_key(&self, name: &str) -> bool {
        self.value.contains_key(name)
    }

    /// Removes the first entry matching `name`, returning the key-value pair.
    /// Uses swap-remove, so the ordering of remaining entries may change.
    pub fn remove_entry(&mut self, name: &str) -> Option<(Key<'de>, Item<'de>)> {
        self.value.remove_entry(name)
    }

    /// Returns a slice of all entries.
    #[inline]
    pub fn entries(&self) -> &[TableEntry<'de>] {
        self.value.entries()
    }
    /// Returns a slice of all entries.
    #[inline]
    pub fn entries_mut(&mut self) -> &mut [TableEntry<'de>] {
        self.value.entries_mut()
    }

    /// Returns an iterator over all entries (key-value pairs).
    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, TableEntry<'de>> {
        self.entries().iter()
    }
}

impl<'a, 'de> IntoIterator for &'a mut Table<'de> {
    type Item = &'a mut (Key<'de>, Item<'de>);
    type IntoIter = std::slice::IterMut<'a, TableEntry<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.entries_mut().iter_mut()
    }
}
impl<'a, 'de> IntoIterator for &'a Table<'de> {
    type Item = &'a (Key<'de>, Item<'de>);
    type IntoIter = std::slice::Iter<'a, TableEntry<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.entries().iter()
    }
}

impl<'de> IntoIterator for Table<'de> {
    type Item = (Key<'de>, Item<'de>);
    type IntoIter = IntoIter<'de>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self.value.0.into_iter())
    }
}

impl<'de> Table<'de> {
    #[inline]
    pub(crate) fn span_start(&self) -> u32 {
        self.meta().span_start()
    }

    #[inline]
    pub(crate) fn set_span_start(&mut self, v: u32) {
        self.meta_mut().set_span_start(v);
    }

    #[inline]
    pub(crate) fn set_span_end(&mut self, v: u32) {
        self.meta_mut().set_span_end(v);
    }

    #[inline]
    pub(crate) fn extend_span_end(&mut self, new_end: u32) {
        self.meta_mut().extend_span_end(new_end);
    }

    #[inline]
    pub(crate) fn set_header_flag(&mut self) {
        self.meta_mut().set_flag(FLAG_HEADER);
    }

    #[inline]
    pub(crate) fn set_dotted_flag(&mut self) {
        self.meta_mut().set_flag(FLAG_DOTTED);
    }

    /// Returns the kind of this table (implicit, dotted, header, or inline).
    #[inline]
    pub fn style(&self) -> TableStyle {
        match self.meta().flag() {
            FLAG_DOTTED => TableStyle::Dotted,
            FLAG_HEADER => TableStyle::Header,
            FLAG_FROZEN => TableStyle::Inline,
            _ => TableStyle::Implicit,
        }
    }

    /// Sets the kind of this table, clearing automatic style resolution.
    #[inline]
    pub fn set_style(&mut self, kind: TableStyle) {
        let flag = match kind {
            TableStyle::Implicit => FLAG_TABLE,
            TableStyle::Dotted => FLAG_DOTTED,
            TableStyle::Header => FLAG_HEADER,
            TableStyle::Inline => FLAG_FROZEN,
        };
        self.meta_mut().set_flag(flag);
        self.meta_mut().clear_auto_style();
    }
}

impl<'de> std::ops::Index<&str> for Table<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: &str) -> &Self::Output {
        if let Some(item) = self.get(index) {
            return MaybeItem::from_ref(item);
        }
        &NONE
    }
}
