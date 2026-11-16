#[cfg(test)]
#[path = "../../tests/unit/item/array.rs"]
mod tests;

use crate::MaybeItem;
use crate::arena::Arena;
use crate::item::{ArrayStyle, FLAG_AOT, FLAG_ARRAY, Item};
use crate::raw::vec::ArenaVec;

/// Arena-backed storage of an array's items.
#[repr(transparent)]
#[derive(Default)]
pub(crate) struct InternalArray<'de>(ArenaVec<'de, Item<'de>>);

impl<'de> InternalArray<'de> {
    #[inline]
    pub(crate) fn new() -> Self {
        Self(ArenaVec::new())
    }

    pub(crate) fn with_capacity(cap: u32, arena: &'de Arena) -> Self {
        Self(ArenaVec::with_capacity(cap, arena))
    }

    pub(crate) fn with_single(value: Item<'de>, arena: &'de Arena) -> Self {
        let mut arr = Self::with_capacity(4, arena);
        arr.push(value, arena);
        arr
    }

    #[inline]
    pub(crate) fn push(&mut self, value: Item<'de>, arena: &'de Arena) {
        self.0.push(value, arena);
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[inline]
    pub(crate) fn get(&self, index: usize) -> Option<&Item<'de>> {
        self.as_slice().get(index)
    }

    #[inline]
    pub(crate) fn get_mut(&mut self, index: usize) -> Option<&mut Item<'de>> {
        self.as_mut_slice().get_mut(index)
    }

    pub(crate) fn remove(&mut self, index: usize) -> Item<'de> {
        self.0.remove(index)
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Option<Item<'de>> {
        self.0.pop()
    }

    #[inline]
    pub(crate) fn last_mut(&mut self) -> Option<&mut Item<'de>> {
        self.as_mut_slice().last_mut()
    }

    #[inline]
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Item<'de>> {
        self.as_slice().iter()
    }

    #[inline]
    pub(crate) fn as_slice(&self) -> &[Item<'de>] {
        self.0.as_slice()
    }

    #[inline]
    pub(crate) fn as_mut_slice(&mut self) -> &mut [Item<'de>] {
        self.0.as_mut_slice()
    }

    /// Deep-clones this array into `arena`. Keys and strings are shared
    /// with the source.
    pub(crate) fn clone_in(&self, arena: &'de Arena) -> Self {
        if self.is_empty() {
            return Self::new();
        }
        Self(self.0.map_in(arena, |item| item.clone_in(arena)))
    }
}

impl std::fmt::Debug for InternalArray<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

impl<'a, 'de> IntoIterator for &'a InternalArray<'de> {
    type Item = &'a Item<'de>;
    type IntoIter = std::slice::Iter<'a, Item<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl<'a, 'de> IntoIterator for &'a mut InternalArray<'de> {
    type Item = &'a mut Item<'de>;
    type IntoIter = std::slice::IterMut<'a, Item<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_mut_slice().iter_mut()
    }
}

impl<'de> IntoIterator for InternalArray<'de> {
    type Item = Item<'de>;
    type IntoIter = crate::raw::vec::IntoIter<'de, Item<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'de> std::ops::Index<usize> for InternalArray<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        if let Some(item) = self.get(index) {
            return MaybeItem::from_ref(item);
        }
        &crate::item::NONE
    }
}

pub use crate::raw::item::Array;

impl<'de> Array<'de> {
    /// Appends a value to the end of the array.
    #[inline]
    pub fn push(&mut self, value: Item<'de>, arena: &'de Arena) {
        self.value.push(value, arena);
    }

    /// Returns the number of elements.
    #[inline]
    pub fn len(&self) -> usize {
        self.value.len()
    }

    /// Returns `true` if the array contains no elements.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Returns a reference to the element at the given index.
    #[inline]
    pub fn get(&self, index: usize) -> Option<&Item<'de>> {
        self.value.get(index)
    }

    /// Returns a mutable reference to the element at the given index.
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Item<'de>> {
        self.value.get_mut(index)
    }

    /// Removes and returns the element at `index`, shifting subsequent
    /// elements to the left.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    #[inline]
    pub fn remove(&mut self, index: usize) -> Item<'de> {
        self.value.remove(index)
    }

    /// Removes and returns the last element, or `None` if empty.
    #[inline]
    pub fn pop(&mut self) -> Option<Item<'de>> {
        self.value.pop()
    }

    /// Returns a mutable reference to the last element.
    #[inline]
    pub fn last_mut(&mut self) -> Option<&mut Item<'de>> {
        self.value.last_mut()
    }

    /// Returns an iterator over references to the elements.
    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, Item<'de>> {
        self.value.iter()
    }

    /// Returns the contents as a slice.
    #[inline]
    pub fn as_slice(&self) -> &[Item<'de>] {
        self.value.as_slice()
    }

    /// Returns the contents as a mutable slice.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [Item<'de>] {
        self.value.as_mut_slice()
    }

    /// Returns the kind of this array (inline or header/array-of-tables).
    #[inline]
    pub fn style(&self) -> ArrayStyle {
        match self.meta().flag() {
            FLAG_AOT => ArrayStyle::Header,
            _ => ArrayStyle::Inline,
        }
    }

    /// Sets the kind of this array, clearing automatic style resolution.
    #[inline]
    pub fn set_style(&mut self, kind: ArrayStyle) {
        let flag = match kind {
            ArrayStyle::Inline => FLAG_ARRAY,
            ArrayStyle::Header => FLAG_AOT,
        };
        self.meta_mut().set_flag(flag);
        self.meta_mut().clear_auto_style();
    }
}

impl<'de> Default for Array<'de> {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Array<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl<'de> std::ops::Index<usize> for Array<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        if let Some(item) = self.value.get(index) {
            return MaybeItem::from_ref(item);
        }
        &crate::item::NONE
    }
}

impl<'a, 'de> IntoIterator for &'a Array<'de> {
    type Item = &'a Item<'de>;
    type IntoIter = std::slice::Iter<'a, Item<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.as_slice().iter()
    }
}

impl<'a, 'de> IntoIterator for &'a mut Array<'de> {
    type Item = &'a mut Item<'de>;
    type IntoIter = std::slice::IterMut<'a, Item<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.as_mut_slice().iter_mut()
    }
}

/// Consuming iterator over an [`Array`], yielding [`Item`]s.
pub struct IntoIter<'de>(crate::raw::vec::IntoIter<'de, Item<'de>>);

impl<'de> Iterator for IntoIter<'de> {
    type Item = Item<'de>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<'de> ExactSizeIterator for IntoIter<'de> {}

impl<'de> IntoIterator for Array<'de> {
    type Item = Item<'de>;
    type IntoIter = IntoIter<'de>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self.value.into_iter())
    }
}
