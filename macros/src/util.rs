use std::alloc::Layout;
use std::mem::MaybeUninit;

const FREAKY_BUCKET_SIZE: usize = 32;

#[repr(C)]
struct Bucket<T> {
    data: [MaybeUninit<T>; FREAKY_BUCKET_SIZE],
    next: *mut Bucket<T>,
}

pub struct MemoryPool<T> {
    current_bucket: *mut Bucket<T>,
    current_size: usize,
}

impl<T> MemoryPool<T> {
    pub fn new() -> Self {
        Self {
            current_size: 0,
            current_bucket: std::ptr::null_mut(),
        }
    }
    pub fn allocator<'a>(&'a mut self) -> Allocator<'a, T> {
        Allocator { inner: self }
    }
}

impl<T> Drop for MemoryPool<T> {
    fn drop(&mut self) {
        let mut current_bucket = self.current_bucket;
        let mut current_size = self.current_size;
        while !current_bucket.is_null() {
            // SAFETY: every non-null bucket in the `next` list was allocated
            // in `alloc_default` with `Layout::new::<Bucket<T>>()` and is
            // freed exactly once here. The newest bucket has `current_size`
            // initialized slots and every older bucket is full, so
            // `drop_in_place` only drops initialized `T`s. No `&mut T`
            // handed out by an `Allocator` outlives the pool's borrow.
            unsafe {
                let bucket = &mut *current_bucket;
                std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(
                    &mut bucket.data as *mut _ as *mut T,
                    current_size,
                ));
                let to_dealloc = current_bucket;
                current_bucket = bucket.next;
                current_size = FREAKY_BUCKET_SIZE;
                std::alloc::dealloc(to_dealloc as *mut u8, Layout::new::<Bucket<T>>());
            }
        }
    }
}

pub struct Allocator<'a, T> {
    inner: &'a mut MemoryPool<T>,
}
impl<'a, T: Default> Allocator<'a, T> {
    pub fn alloc_default(&mut self) -> &'a mut T {
        if self.inner.current_bucket.is_null() || self.inner.current_size >= FREAKY_BUCKET_SIZE {
            // SAFETY: `Bucket<T>` has a non-zero size (it holds a pointer), so
            // `alloc` is called with a valid layout; the result is checked
            // for null. `next` is written through a raw field offset before
            // any reference to the uninitialized bucket is created.
            unsafe {
                let ptr = std::alloc::alloc(Layout::new::<Bucket<T>>()) as *mut Bucket<T>;
                if ptr.is_null() {
                    panic!();
                }
                ptr.byte_add(std::mem::offset_of!(Bucket<T>, next))
                    .cast::<*mut Bucket<T>>()
                    .write(self.inner.current_bucket);

                self.inner.current_bucket = ptr;
                self.inner.current_size = 0;
            }
        }
        // SAFETY: the bucket is non-null and `current_size <
        // FREAKY_BUCKET_SIZE` (a new bucket was started above otherwise).
        // `Bucket` is repr(C) with `data` first, so the bucket pointer is
        // the first `MaybeUninit<T>` slot and `tail` is an unused slot in
        // bounds. Each slot is handed out once, so the returned `&'a mut T`
        // values never alias; they live no longer than the pool borrow.
        unsafe {
            let entries = self.inner.current_bucket as *mut T;
            let tail = entries.add(self.inner.current_size);
            tail.write(<T as Default>::default());
            self.inner.current_size += 1;
            &mut *tail
        }
    }
}
