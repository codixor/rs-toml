use std::alloc::Layout;
use std::cell::Cell;
use std::ptr::{self, NonNull};

const HEADER_SIZE: usize = std::mem::size_of::<SlabHeader>();
const INITIAL_SLAB_SIZE: usize = 4096;
const ALLOC_ALIGN: usize = 8;
const SLAB_ALIGN: usize = if std::mem::align_of::<SlabHeader>() >= ALLOC_ALIGN {
    std::mem::align_of::<SlabHeader>()
} else {
    ALLOC_ALIGN
};

const _: () = assert!(SLAB_ALIGN >= ALLOC_ALIGN);
const _: () = assert!(HEADER_SIZE.is_multiple_of(ALLOC_ALIGN));

#[repr(C)]
struct SlabHeader {
    prev: Option<NonNull<SlabHeader>>,
    size: usize,
}

// SAFETY: The only `SlabHeader` that needs `Sync` is the static `EMPTY_SLAB`
// sentinel. It is immutable (prev=None, size=0) and never written through the
// pointer. Heap-allocated SlabHeaders are only reachable through `Arena`, which
// is `!Sync` (contains `Cell`), so they are never shared across threads.
unsafe impl Sync for SlabHeader {}

static EMPTY_SLAB: SlabHeader = SlabHeader {
    prev: None,
    size: 0,
};

/// Bump allocator backing all data structures produced by the parser.
///
/// Create an `Arena` before calling [`parse`](crate::parse) and pass it by
/// reference. The arena must outlive the returned [`Document`](crate::Document)
/// because parsed values borrow from it. All memory is freed on drop.
///
/// # Examples
///
/// ```
/// let arena = rs_toml::Arena::new();
/// let table = rs_toml::parse("key = \"value\"", &arena)?;
/// // `table` borrows from both the input string and `arena`.
/// # Ok::<(), rs_toml::Error>(())
/// ```
pub struct Arena {
    ptr: Cell<NonNull<u8>>,
    end: Cell<NonNull<u8>>,
    slab: Cell<NonNull<SlabHeader>>,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<Arena>() == 24);

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl Arena {
    /// Creates a new, empty arena.
    pub fn new() -> Self {
        // Safety: EMPTY_SLAB is a static with a stable address.
        let sentinel =
            unsafe { NonNull::new_unchecked(&EMPTY_SLAB as *const SlabHeader as *mut SlabHeader) };
        let dangling = NonNull::dangling();
        Arena {
            ptr: Cell::new(dangling),
            end: Cell::new(dangling),
            slab: Cell::new(sentinel),
        }
    }

    /// Allocate `size` bytes aligned to 8.
    ///
    /// Returns a non-null pointer to the allocated region. Aborts on OOM.
    #[inline]
    pub(crate) fn alloc(&self, size: usize) -> NonNull<u8> {
        let ptr = self.ptr.get();
        let addr = ptr.as_ptr() as usize;
        let aligned_addr = (addr + ALLOC_ALIGN - 1) & !(ALLOC_ALIGN - 1);
        let padding = aligned_addr - addr;
        let needed = padding.saturating_add(size);
        let remaining = self.end.get().as_ptr() as usize - addr;

        let result = if needed <= remaining {
            // Safety: needed bytes fit within the current slab (end >= ptr is
            // invariant, so remaining cannot underflow). Using add() from ptr
            // preserves provenance.
            unsafe {
                self.ptr
                    .set(NonNull::new_unchecked(ptr.as_ptr().add(needed)));
                NonNull::new_unchecked(ptr.as_ptr().add(padding))
            }
        } else {
            self.alloc_slow(size)
        };

        // Tag pointer for MIRI
        #[cfg(test)]
        if true {
            use std::mem::MaybeUninit;

            // SAFETY: `result` points to `size` freshly reserved bytes of the
            // current slab (both branches above guarantee it), so a
            // `MaybeUninit<u8>` slice over them is valid; its pointer is
            // non-null because `result` is.
            unsafe {
                return NonNull::new_unchecked(
                    std::slice::from_raw_parts_mut(result.cast::<MaybeUninit<u8>>().as_ptr(), size)
                        .as_mut_ptr()
                        .cast(),
                );
            }
        }

        result
    }

    /// Grow an existing allocation in place when possible, otherwise allocate
    /// a new region and copy the old data.
    ///
    /// Returns the (possibly unchanged) pointer to the allocation.
    ///
    /// # Safety
    ///
    /// - `ptr` must have been returned by a prior `alloc` on this arena whose
    ///   size was `old_size`.
    /// - `new_size` must be `>= old_size`. Violating this causes wrapping
    ///   underflow in the in place growth path or an out of bounds copy.
    pub(crate) unsafe fn realloc(
        &self,
        ptr: NonNull<u8>,
        old_size: usize,
        new_size: usize,
    ) -> NonNull<u8> {
        debug_assert!(new_size >= old_size);

        // Use integer arithmetic for bounds checks to avoid creating
        // out-of-bounds pointers and to preserve strict provenance.
        let old_end = ptr.as_ptr() as usize + old_size;
        let head = self.ptr.get().as_ptr() as usize;

        if old_end == head {
            // Compare as new_size <= end - ptr instead of ptr + new_size <= end
            // to avoid wrapping overflow when new_size is very large.
            let remaining = self.end.get().as_ptr() as usize - ptr.as_ptr() as usize;
            if new_size <= remaining {
                let extra = new_size - old_size;
                // SAFETY: `old_end == head`, so this allocation is the last
                // one in the current slab and `head + extra = ptr + new_size`
                // stays within the slab (`new_size <= remaining`). Deriving
                // from the bump pointer preserves provenance; `head - old_size
                // = ptr` is in bounds of the same slab.
                unsafe {
                    let head_ptr = self.ptr.get().as_ptr();
                    self.ptr.set(NonNull::new_unchecked(head_ptr.add(extra)));
                    // Re-derive from the bump pointer (which carries full slab
                    // provenance) instead of returning `ptr` directly, because
                    // the MIRI tagging in alloc() restricts `ptr`'s Stacked
                    // Borrows tag to old_size bytes.
                    let result = NonNull::new_unchecked(head_ptr.sub(old_size));

                    #[cfg(test)]
                    if true {
                        use std::mem::MaybeUninit;
                        return NonNull::new_unchecked(
                            std::slice::from_raw_parts_mut(
                                result.cast::<MaybeUninit<u8>>().as_ptr(),
                                new_size,
                            )
                            .as_mut_ptr()
                            .cast(),
                        );
                    }

                    return result;
                }
            }
        }

        let new_ptr = self.alloc(new_size);
        // SAFETY:
        // - Source (`ptr`) is from a prior alloc of `old_size` bytes, valid to read.
        // - Destination (`new_ptr`) is a fresh alloc of `new_size >= old_size` bytes.
        // - The regions do not overlap because `new_ptr` comes from a subsequent
        //   bump (same or different slab) so it cannot alias the `ptr` region.
        unsafe { ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr(), old_size) };
        new_ptr
    }

    #[cold]
    #[inline(never)]
    fn alloc_slow(&self, size: usize) -> NonNull<u8> {
        self.grow(size);

        let ptr = self.ptr.get();
        let addr = ptr.as_ptr() as usize;
        let aligned_addr = (addr + ALLOC_ALIGN - 1) & !(ALLOC_ALIGN - 1);
        let padding = aligned_addr - addr;
        // Cannot overflow: grow() panics if HEADER_SIZE + size overflows,
        // and post-grow ptr is SLAB_ALIGN-aligned so padding is always 0.
        let needed = padding + size;
        debug_assert!(needed <= self.end.get().as_ptr() as usize - addr);

        // Safety: grow() guarantees the new slab is large enough.
        // Using add() from ptr preserves provenance.
        unsafe {
            self.ptr
                .set(NonNull::new_unchecked(ptr.as_ptr().add(needed)));
            NonNull::new_unchecked(ptr.as_ptr().add(padding))
        }
    }

    /// Create a scratch buffer that writes into the arena's current slab.
    ///
    /// The scratch reserves the rest of the current slab by moving the bump
    /// pointer to the slab end, so allocations made while it is alive come
    /// from a new slab and can never overlap the scratch bytes. Committing or
    /// dropping the scratch hands the unused tail back when nothing was
    /// allocated in between.
    pub(crate) fn scratch(&self) -> Scratch<'_> {
        let start = self.ptr.get();
        let end = self.end.get();
        let cap = end.as_ptr() as usize - start.as_ptr() as usize;
        self.ptr.set(end);
        Scratch {
            arena: self,
            start,
            len: 0,
            cap,
        }
    }

    /// Moves `value` into the arena and returns a reference to it.
    ///
    /// `T: Copy` guarantees there is no destructor to run: the arena never
    /// drops its contents.
    #[allow(clippy::mut_from_ref, reason = "every call returns a fresh allocation")]
    pub(crate) fn alloc_copy<T: Copy>(&self, value: T) -> &mut T {
        const { assert!(std::mem::align_of::<T>() <= ALLOC_ALIGN) };
        let ptr = self.alloc(std::mem::size_of::<T>()).cast::<T>();
        // SAFETY: `ptr` is a fresh allocation of `size_of::<T>()` bytes,
        // aligned to ALLOC_ALIGN >= align_of::<T>() (const-asserted), and
        // disjoint from every other live allocation, so writing a `T` and
        // handing out a unique reference bounded by `&self` is sound.
        unsafe {
            ptr.as_ptr().write(value);
            &mut *ptr.as_ptr()
        }
    }

    /// Allocates a slice holding a copy of `prefix` followed by `last`.
    pub(crate) fn alloc_extended<T: Copy>(&self, prefix: &[T], last: T) -> &[T] {
        const { assert!(std::mem::align_of::<T>() <= ALLOC_ALIGN) };
        let len = prefix.len() + 1;
        let size = len
            .checked_mul(std::mem::size_of::<T>())
            .expect("layout overflow");
        let ptr = self.alloc(size).cast::<T>().as_ptr();
        // SAFETY: `ptr` is a fresh, suitably aligned allocation of `len`
        // elements, disjoint from `prefix`. The first `prefix.len()`
        // elements are copied from `prefix` and the last is written, so all
        // `len` elements are initialized before the slice is created.
        unsafe {
            std::ptr::copy_nonoverlapping(prefix.as_ptr(), ptr, prefix.len());
            ptr.add(prefix.len()).write(last);
            std::slice::from_raw_parts(ptr, len)
        }
    }

    /// Allocates a slice of `len` copies of `value`.
    #[allow(clippy::mut_from_ref, reason = "every call returns a fresh allocation")]
    pub(crate) fn alloc_filled<T: Copy>(&self, len: usize, value: T) -> &mut [T] {
        const { assert!(std::mem::align_of::<T>() <= ALLOC_ALIGN) };
        let size = len
            .checked_mul(std::mem::size_of::<T>())
            .expect("layout overflow");
        let ptr = self.alloc(size).cast::<T>().as_ptr();
        // SAFETY: `ptr` is a fresh, suitably aligned allocation of `len`
        // elements; every element is written before the slice is created.
        unsafe {
            for i in 0..len {
                ptr.add(i).write(value);
            }
            std::slice::from_raw_parts_mut(ptr, len)
        }
    }

    fn grow(&self, size: usize) {
        // SAFETY: self.slab always points to a valid SlabHeader — either the
        // static EMPTY_SLAB sentinel or a heap-allocated header written in grow().
        let current_size = unsafe { self.slab.get().as_ref().size };

        let min_slab = HEADER_SIZE.checked_add(size).expect("layout overflow");

        let new_size = current_size
            .saturating_mul(2)
            .max(min_slab)
            .max(INITIAL_SLAB_SIZE);

        let slab_layout =
            Layout::from_size_align(new_size, SLAB_ALIGN).expect("slab layout overflow");

        // SAFETY: slab_layout was validated by Layout::from_size_align above.
        let raw = unsafe { std::alloc::alloc(slab_layout) };
        let Some(base) = NonNull::new(raw) else {
            std::alloc::handle_alloc_error(slab_layout);
        };

        // Safety: base points to a freshly allocated region of new_size bytes.
        unsafe {
            let header_ptr = base.as_ptr().cast::<SlabHeader>();
            header_ptr.write(SlabHeader {
                prev: Some(self.slab.get()),
                size: new_size,
            });

            self.slab.set(NonNull::new_unchecked(header_ptr));
            self.ptr
                .set(NonNull::new_unchecked(base.as_ptr().add(HEADER_SIZE)));
            self.end
                .set(NonNull::new_unchecked(base.as_ptr().add(new_size)));
        }
    }

    /// Allocates a copy of `s` in the arena and returns a reference to it.
    pub fn alloc_str(&self, s: &str) -> &str {
        if s.is_empty() {
            return "";
        }
        let dst = self.alloc(s.len());
        // SAFETY: dst is a fresh arena allocation of s.len() bytes, disjoint
        // from s. The resulting slice is valid UTF-8 because s is.
        unsafe {
            std::ptr::copy_nonoverlapping(s.as_ptr(), dst.as_ptr(), s.len());
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(dst.as_ptr(), s.len()))
        }
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        let mut current = self.slab.get();
        loop {
            // Safety: current is either a heap slab or the static sentinel.
            let header = unsafe { current.as_ref() };
            if header.size == 0 {
                break;
            }
            let prev = header.prev;
            // SAFETY: header.size and SLAB_ALIGN are the values grow()
            // validated with Layout::from_size_align for this slab.
            let slab_layout = unsafe { Layout::from_size_align_unchecked(header.size, SLAB_ALIGN) };
            // SAFETY: `current` is a heap slab (size != 0, so not the static
            // sentinel) allocated in grow() with exactly `slab_layout`, and
            // it is freed once: we follow `prev` links, which form a list.
            unsafe {
                std::alloc::dealloc(current.as_ptr().cast(), slab_layout);
            }
            match prev {
                Some(p) => current = p,
                None => break,
            }
        }
    }
}

/// A temporary UTF-8 buffer that writes directly into an [`Arena`] slab.
///
/// Creating a scratch reserves the rest of the current slab (the arena's
/// bump pointer moves to the slab end), so other allocations made while the
/// scratch is alive come from a new slab and never alias it. On
/// [`commit`](Scratch::commit) the written bytes stay allocated and the
/// unused tail is returned; on drop without commit the whole reservation is
/// returned. In both cases the tail is only returned when nothing else was
/// allocated in between (the bump pointer is still at the reservation end).
///
/// # Invariant
///
/// `start..start + cap` is the reserved region of one slab, `len <= cap`,
/// and `start..start + len` holds initialized, valid UTF-8 (every writer
/// appends whole `str`s, `char`s or ASCII bytes).
pub(crate) struct Scratch<'a> {
    arena: &'a Arena,
    start: NonNull<u8>,
    len: usize,
    cap: usize,
}

impl<'a> Scratch<'a> {
    /// Appends raw bytes that the caller's API guarantees form valid UTF-8
    /// at a character boundary.
    #[inline]
    fn extend_bytes(&mut self, bytes: &[u8]) {
        if bytes.len() > self.cap - self.len {
            self.grow_slow(bytes.len());
        }
        // SAFETY: cap - len >= bytes.len(), so the copy stays inside the
        // reserved region; the source cannot overlap it because the region
        // is not reachable through any other reference.
        unsafe {
            ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.start.as_ptr().add(self.len),
                bytes.len(),
            );
        }
        self.len += bytes.len();
    }

    /// Appends an ASCII byte. Non-ASCII input is replaced by `?` so the
    /// buffer always stays valid UTF-8 (callers only pass ASCII).
    #[inline]
    pub fn push(&mut self, byte: u8) {
        debug_assert!(byte.is_ascii());
        let byte = if byte.is_ascii() { byte } else { b'?' };
        let len = self.len;
        if len == self.cap {
            self.grow_slow(1);
        }
        // SAFETY: len < cap, so start + len is within the reserved region.
        unsafe {
            self.start.as_ptr().add(len).write(byte);
        }
        self.len = len + 1;
    }

    /// Appends a character as UTF-8.
    #[inline]
    pub fn push_char(&mut self, ch: char) {
        let mut buf = [0u8; 4];
        self.extend_bytes(ch.encode_utf8(&mut buf).as_bytes());
    }

    /// Appends a string.
    #[inline]
    pub fn extend(&mut self, text: &str) {
        self.extend_bytes(text.as_bytes());
    }

    /// Push text while stripping underscores. Returns `false` if any
    /// underscore is not placed between two ASCII digits.
    ///
    /// Removing ASCII `_` bytes from valid UTF-8 keeps it valid UTF-8.
    #[inline]
    pub(crate) fn push_strip_underscores(&mut self, text: &str) -> bool {
        let mut prev = 0u8;
        for &b in text.as_bytes() {
            if b == b'_' {
                if !prev.is_ascii_digit() {
                    return false;
                }
            } else {
                if prev == b'_' && !b.is_ascii_digit() {
                    return false;
                }
                if b.is_ascii() {
                    self.push(b);
                } else {
                    self.extend_bytes(&[b]);
                }
            }
            prev = b;
        }
        prev != b'_'
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        if self.len == 0 {
            return &[];
        }
        // SAFETY: start..start+len was written by us and is within the slab.
        unsafe { std::slice::from_raw_parts(self.start.as_ptr(), self.len) }
    }

    /// The written text.
    #[inline]
    pub fn as_str(&self) -> &str {
        // SAFETY: the buffer only ever receives whole `str`s, `char`s and
        // ASCII bytes (struct invariant), so it is valid UTF-8.
        unsafe { std::str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// Finalize the scratch data and return it as a string tied to the
    /// arena's lifetime. The written bytes stay allocated.
    pub fn commit(self) -> &'a str {
        let len = self.len;
        let start = self.start;
        let reserved_end = start.as_ptr() as usize + self.cap;
        let arena = self.arena;
        std::mem::forget(self);
        if arena.ptr.get().as_ptr() as usize == reserved_end {
            // SAFETY: start + len <= start + cap, inside the reserved region
            // of the current slab; deriving from `start` keeps provenance.
            arena
                .ptr
                .set(unsafe { NonNull::new_unchecked(start.as_ptr().add(len)) });
        }
        if len == 0 {
            return "";
        }
        // SAFETY: start..start+len is initialized, valid UTF-8 (struct
        // invariant) and is never written again: it is now below the bump
        // pointer, or in a slab the arena has moved past.
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(start.as_ptr(), len)) }
    }

    #[cold]
    #[inline(never)]
    fn grow_slow(&mut self, additional: usize) {
        let required = self.len.checked_add(additional).expect("scratch overflow");
        let new_cap = self.cap.saturating_mul(2).max(required);

        self.arena.grow(new_cap);

        // Copy existing scratch data to the start of the new slab.
        let new_start = self.arena.ptr.get();
        let new_end = self.arena.end.get();
        if self.len > 0 {
            // SAFETY: the old data at start..+len is still valid (slabs are
            // only freed when the arena drops). The new slab has at least
            // new_cap >= required > len bytes and is disjoint from the old.
            unsafe {
                ptr::copy_nonoverlapping(self.start.as_ptr(), new_start.as_ptr(), self.len);
            }
        }
        self.start = new_start;
        self.cap = new_end.as_ptr() as usize - new_start.as_ptr() as usize;
        // Reserve the new slab.
        self.arena.ptr.set(new_end);
    }
}

impl Drop for Scratch<'_> {
    fn drop(&mut self) {
        // Return the reservation if nothing was allocated meanwhile.
        if self.arena.ptr.get().as_ptr() as usize == self.start.as_ptr() as usize + self.cap {
            self.arena.ptr.set(self.start);
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/arena.rs"]
#[allow(
    clippy::undocumented_unsafe_blocks,
    reason = "tests call the unsafe arena API directly to exercise its contract"
)]
mod tests;
