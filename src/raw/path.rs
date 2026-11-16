//! Owned, lazily computed error paths.
//!
//! [`MaybeTomlPath`] packs three states into 16 bytes: no path, the address
//! of the [`Item`] an error was reported on (resolved to a path after
//! deserialization), or an owned heap copy of the path whose key names point
//! into the same allocation. The self-referential copy is why this lives in
//! `raw`; every function here is safe to call with any arguments.

use crate::Item;
use crate::error::{PathComponent, TomlPath};
use crate::Key;
use std::ptr::NonNull;

pub(crate) struct MaybeTomlPath {
    ptr: NonNull<PathComponent<'static>>,
    len: u32,
    size: u32,
}

impl MaybeTomlPath {
    pub(crate) fn empty() -> Self {
        MaybeTomlPath {
            ptr: NonNull::dangling(),
            len: u32::MAX,
            size: 0,
        }
    }

    pub(crate) fn has_path(&self) -> bool {
        self.size > 0
    }

    pub(crate) fn from_components(components: &[PathComponent<'_>]) -> MaybeTomlPath {
        if components.is_empty() {
            return Self::empty();
        }

        let len = components.len();
        let mut total_string_bytes: usize = 0;
        for comp in components {
            if let PathComponent::Key(key) = comp {
                total_string_bytes += key.name.len();
            }
        }

        let comp_size = len
            .checked_mul(std::mem::size_of::<PathComponent<'static>>())
            .expect("error path too large");
        let size = comp_size
            .checked_add(total_string_bytes)
            .expect("error path too large");
        // `len` and `size` are stored as u32 and `Drop` rebuilds the layout
        // from `size`, so they must round-trip exactly.
        let len32 = u32::try_from(len).expect("error path too large");
        let size32 = u32::try_from(size).expect("error path too large");

        // SAFETY: size > 0 because len >= 1 and size_of::<PathComponent>() > 0.
        let layout = std::alloc::Layout::from_size_align(
            size,
            std::mem::align_of::<PathComponent<'static>>(),
        )
        .unwrap();
        // SAFETY: layout has non-zero size
        let raw = unsafe { std::alloc::alloc(layout) };
        if raw.is_null() {
            std::alloc::handle_alloc_error(layout);
        }

        let base = raw.cast::<PathComponent<'static>>();
        // SAFETY: `raw` is an allocation of `size = comp_size +
        // total_string_bytes` bytes, so `raw + comp_size` is in bounds (or
        // one past the end when there are no key bytes).
        let mut string_cursor = unsafe { raw.add(comp_size) };

        for (i, comp) in components.iter().enumerate() {
            let stored = match comp {
                PathComponent::Key(key) => {
                    let name_bytes = key.name.as_bytes();
                    let name_len = name_bytes.len();
                    // SAFETY: string_cursor points into the trailing region we allocated
                    unsafe {
                        std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), string_cursor, name_len);
                    }
                    // SAFETY: we just wrote valid UTF-8 bytes here
                    let name: &'static str = unsafe {
                        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                            string_cursor,
                            name_len,
                        ))
                    };
                    // SAFETY: the trailing region holds the sum of all key
                    // lengths; advancing past this key stays in bounds (at
                    // most one past the end after the last key).
                    string_cursor = unsafe { string_cursor.add(name_len) };
                    PathComponent::Key(Key {
                        name,
                        span: key.span,
                    })
                }
                PathComponent::Index(idx) => PathComponent::Index(*idx),
            };
            // SAFETY: we allocated space for `len` PathComponents
            unsafe {
                base.add(i).write(stored);
            }
        }

        MaybeTomlPath {
            // SAFETY: raw was checked non-null above
            ptr: unsafe { NonNull::new_unchecked(base) },
            len: len32,
            size: size32,
        }
    }
    /// Records the address of `item` so the path can be computed later by
    /// locating the item in the parsed tree. The address is never
    /// dereferenced.
    #[inline(always)]
    pub(crate) fn uncomputed(item: &Item<'_>) -> Self {
        MaybeTomlPath {
            ptr: NonNull::from(item).cast::<PathComponent<'static>>(),
            len: 0,
            size: 0,
        }
    }

    pub(crate) fn is_uncomputed(&self) -> bool {
        self.size == 0 && self.len != u32::MAX
    }

    /// The address recorded by [`uncomputed`](Self::uncomputed). Callers
    /// only compare its address; it must never be dereferenced.
    pub(crate) fn uncomputed_ptr(&self) -> *const () {
        self.ptr.as_ptr() as *const ()
    }

    pub(crate) fn as_toml_path<'a>(&'a self) -> Option<&'a TomlPath<'a>> {
        if !self.has_path() {
            return None;
        }
        // SAFETY: components live in the allocation, strings point into
        // the same allocation. The returned TomlPath borrows self, so the
        // inner 'static is shortened to 'a, preventing it from escaping.
        let slice = unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len as usize) };
        // SAFETY: `TomlPath` is `#[repr(transparent)]` over
        // `[PathComponent]`, so the cast keeps layout and metadata (length).
        Some(unsafe { &*(slice as *const [PathComponent<'static>] as *const TomlPath<'a>) })
    }
}

impl Drop for MaybeTomlPath {
    fn drop(&mut self) {
        let size = self.size as usize;
        if size > 0 {
            let layout = std::alloc::Layout::from_size_align(
                size,
                std::mem::align_of::<PathComponent<'static>>(),
            )
            .unwrap();
            // SAFETY: ptr was allocated with this layout
            unsafe {
                std::alloc::dealloc(self.ptr.as_ptr() as *mut u8, layout);
            }
        }
    }
}

// SAFETY: `MaybeTomlPath` is in one of three states:
// - empty (`len == u32::MAX`): `ptr` is dangling and never used.
// - computed (`size > 0`): `ptr` is a heap allocation owned exclusively by
//   this value (component array plus the key bytes it points into); it is
//   freed only in `Drop` and read only through `&self`, so moving it to
//   another thread or sharing `&self` is like doing so with a `Box<[u8]>`.
// - uncomputed (`size == 0`, `len == 0`): `ptr` is the address of an
//   `Item` borrowed from a parsed tree. It is NOT owned and may dangle once
//   the arena is gone. It is never dereferenced: `as_toml_path` and `Drop`
//   ignore it (`has_path()` and `size > 0` are false), and the only reader,
//   `uncomputed_ptr`, feeds plain address (`usize`) comparisons in
//   `de::compute_paths` and `helper`. An address carries no
//   thread affinity, so sending or sharing it cannot cause a data race.
unsafe impl Send for MaybeTomlPath {}
// SAFETY: see `Send` above; `&MaybeTomlPath` only allows reading the owned
// allocation or copying the opaque address.
unsafe impl Sync for MaybeTomlPath {}
