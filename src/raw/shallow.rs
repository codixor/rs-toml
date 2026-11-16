//! A read-only table of shallow item copies used by `flatten_any`.

use crate::{Arena, Item, Key, Table};
use std::marker::PhantomData;

/// Accumulates bitwise copies of items borrowed from a parent table so they
/// can be handed to [`FromToml`](crate::FromToml) as one table.
///
/// # Invariant
///
/// Every entry is a shallow copy of an `Item` that stays immutably borrowed
/// for `'b`, so the copies share arena storage with live originals. That
/// storage is never mutated while `'b` lasts (the originals are behind `&`)
/// and the copies are only reachable through [`table`](Self::table), which
/// returns a shared reference, so neither side can observe a write through
/// the other. `Item` has no `Drop`, so dropping the copies frees nothing.
#[doc(hidden)]
pub struct ShallowTable<'de, 'b> {
    table: Table<'de>,
    marker: PhantomData<&'b Item<'de>>,
}

impl<'de, 'b> ShallowTable<'de, 'b> {
    pub(crate) fn new() -> Self {
        ShallowTable {
            table: Table::new(),
            marker: PhantomData,
        }
    }

    /// Appends a shallow copy of `item` under `key`.
    pub(crate) fn insert(&mut self, key: Key<'de>, item: &'b Item<'de>, arena: &'de Arena) {
        // SAFETY: `Item` has no `Drop`, so the bitwise copy does not cause a
        // double free. The copy aliases arena storage owned by `item`, which
        // stays immutably borrowed for 'b; the copy is only exposed through
        // `&Table` (see the type invariant), so no write can reach shared
        // storage through either value. The arena outlives 'de.
        let shallow = unsafe { std::ptr::read(item) };
        self.table.insert_unique(key, shallow, arena);
    }

    /// The accumulated entries, read-only.
    pub(crate) fn table(&self) -> &Table<'de> {
        &self.table
    }
}
