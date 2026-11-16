//! A growable vector whose storage lives in an [`Arena`].

use crate::arena::Arena;
use std::marker::PhantomData;
use std::ptr::NonNull;

/// A vector of `T` backed by arena memory, 16 bytes (`len`, `cap`, `ptr`).
///
/// Elements are never dropped: the arena frees its slabs wholesale, so `T`
/// should not own resources (a `T` with `Drop` simply leaks, which is safe).
///
/// # Invariant
///
/// `len <= cap`. When `cap > 0`, `ptr` points to `cap` slots of `T` in an
/// arena that lives for `'a`, and the first `len` slots are initialized.
/// When `cap == 0`, `ptr` is dangling (well aligned) and `len == 0`.
#[repr(C, align(8))]
pub(crate) struct ArenaVec<'a, T> {
    len: u32,
    cap: u32,
    ptr: NonNull<T>,
    marker: PhantomData<&'a Arena>,
}

impl<'a, T> ArenaVec<'a, T> {
    const ALIGN_OK: () = assert!(std::mem::align_of::<T>() <= 8);

    #[inline]
    pub(crate) const fn new() -> Self {
        Self {
            len: 0,
            cap: 0,
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    pub(crate) fn with_capacity(cap: u32, arena: &'a Arena) -> Self {
        let mut vec = Self::new();
        if cap > 0 {
            vec.grow_to(cap, arena);
        }
        vec
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.len as usize
    }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Appends `value` and returns a reference to it.
    #[inline]
    pub(crate) fn push(&mut self, value: T, arena: &'a Arena) -> &mut T {
        let len = self.len;
        if len == self.cap {
            self.grow(arena);
        }
        // SAFETY: after grow, len < cap, so slot `len` is inside the
        // allocation (invariant); it is uninitialized and becomes the new
        // last element.
        unsafe {
            let slot = self.ptr.as_ptr().add(len as usize);
            slot.write(value);
            self.len = len + 1;
            &mut *slot
        }
    }

    #[inline]
    pub(crate) fn as_slice(&self) -> &[T] {
        // SAFETY: the first `len` slots are initialized (invariant); for
        // len == 0 the dangling pointer is aligned and non-null.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len as usize) }
    }

    #[inline]
    pub(crate) fn as_mut_slice(&mut self) -> &mut [T] {
        // SAFETY: as in `as_slice`; `&mut self` makes the borrow unique.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len as usize) }
    }

    /// Removes the last element.
    #[inline]
    pub(crate) fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: slot `len` (after the decrement) was initialized and is now
        // outside the live range, so it is moved out exactly once.
        Some(unsafe { self.ptr.as_ptr().add(self.len as usize).read() })
    }

    /// Removes the element at `index`, shifting later elements left.
    ///
    /// # Panics
    ///
    /// Panics if `index >= len`.
    pub(crate) fn remove(&mut self, index: usize) -> T {
        let len = self.len as usize;
        assert!(index < len, "index out of bounds");
        // SAFETY: index < len, so the slot is initialized; it is moved out,
        // the tail `index + 1..len` is shifted down by one within the same
        // allocation, and the length shrinks, so no value is duplicated.
        unsafe {
            let slot = self.ptr.as_ptr().add(index);
            let removed = slot.read();
            std::ptr::copy(slot.add(1), slot, len - index - 1);
            self.len -= 1;
            removed
        }
    }

    /// Removes the element at `index`, replacing it with the last element.
    ///
    /// # Panics
    ///
    /// Panics if `index >= len`.
    pub(crate) fn swap_remove(&mut self, index: usize) -> T {
        let last = self.len().checked_sub(1).expect("index out of bounds");
        self.as_mut_slice().swap(index, last);
        match self.pop() {
            Some(value) => value,
            None => unreachable!("len was checked to be non-zero"),
        }
    }

    /// Builds a new vector of the same length from `f(element)`, allocated
    /// exactly (`cap == len`) in `arena`.
    pub(crate) fn map_in(&self, arena: &'a Arena, mut f: impl FnMut(&T) -> T) -> Self {
        let mut out = Self::with_capacity(self.len, arena);
        for value in self.as_slice() {
            // Capacity was reserved above, so this never reallocates.
            out.push(f(value), arena);
        }
        out
    }

    #[cold]
    fn grow(&mut self, arena: &'a Arena) {
        let new_cap = if self.cap == 0 {
            // Tables start at 2 entries; arrays at 4 items (24-byte items).
            if std::mem::size_of::<T>() > 24 { 2 } else { 4 }
        } else {
            self.cap.checked_mul(2).expect("capacity overflow")
        };
        self.grow_to(new_cap, arena);
    }

    fn grow_to(&mut self, new_cap: u32, arena: &'a Arena) {
        let () = Self::ALIGN_OK;
        let new_size = (new_cap as usize)
            .checked_mul(std::mem::size_of::<T>())
            .expect("capacity overflow");
        self.ptr = if self.cap > 0 {
            let old_size = self.cap as usize * std::mem::size_of::<T>();
            // SAFETY: `ptr` is an arena allocation of `cap` slots that lives
            // for 'a (invariant), so it is readable for `old_size` bytes, and
            // new_size >= old_size because callers only grow.
            unsafe { arena.realloc(self.ptr.cast(), old_size, new_size).cast() }
        } else {
            arena.alloc(new_size).cast()
        };
        self.cap = new_cap;
    }
}

impl<T> Default for ArenaVec<'_, T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Consuming iterator over an [`ArenaVec`], yielding elements by value.
pub(crate) struct IntoIter<'a, T> {
    vec: ArenaVec<'a, T>,
    index: u32,
}

impl<T> Iterator for IntoIter<'_, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<T> {
        if self.index < self.vec.len {
            // SAFETY: index < len, so the slot is initialized; the index
            // advances past it, so each element is moved out exactly once
            // (the consumed vector is never read again).
            let value = unsafe { self.vec.ptr.as_ptr().add(self.index as usize).read() };
            self.index += 1;
            Some(value)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.vec.len - self.index) as usize;
        (remaining, Some(remaining))
    }
}

impl<T> ExactSizeIterator for IntoIter<'_, T> {}

impl<'a, T> IntoIterator for ArenaVec<'a, T> {
    type Item = T;
    type IntoIter = IntoIter<'a, T>;

    fn into_iter(self) -> IntoIter<'a, T> {
        IntoIter {
            vec: self,
            index: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ArenaVec;
    use crate::arena::Arena;

    #[test]
    fn push_remove_pop_and_iterate() {
        let arena = Arena::new();
        let mut v: ArenaVec<'_, u64> = ArenaVec::new();
        for i in 0..100 {
            *v.push(i, &arena) += 1;
        }
        assert_eq!(v.len(), 100);
        assert_eq!(v.remove(0), 1);
        assert_eq!(v.swap_remove(0), 2);
        assert_eq!(v.as_slice()[0], 100);
        assert_eq!(v.pop(), Some(99));
        let doubled = v.map_in(&arena, |x| x * 2);
        assert_eq!(doubled.len(), 97);
        let collected: Vec<u64> = v.into_iter().collect();
        assert_eq!(collected.len(), 97);
    }

    #[test]
    fn foreign_arena_growth_copies() {
        let a = Arena::new();
        let b = Arena::new();
        let mut v: ArenaVec<'_, u32> = ArenaVec::new();
        for i in 0..10 {
            v.push(i, if i % 2 == 0 { &a } else { &b });
        }
        assert_eq!(v.as_slice(), &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }
}
