const FREAKY_BUCKET_SIZE: usize = 32;

/// A chunk of default-initialized slots, linked to the next (older-to-newer)
/// chunk. Slots are handed out at most once each.
struct Bucket<T> {
    data: Vec<T>,
    next: Option<Box<Bucket<T>>>,
}

/// Owns the buckets that an [`Allocator`] hands out `&'a mut T` slots from.
pub struct MemoryPool<T> {
    head: Option<Box<Bucket<T>>>,
}

impl<T> MemoryPool<T> {
    pub fn new() -> Self {
        Self { head: None }
    }
    pub fn allocator<'a>(&'a mut self) -> Allocator<'a, T> {
        Allocator {
            remaining: &mut [],
            next: Some(&mut self.head),
        }
    }
}

/// Hands out unique `&'a mut T` slots from a [`MemoryPool`] by splitting
/// borrowed buckets, so no slot is ever aliased.
pub struct Allocator<'a, T> {
    remaining: &'a mut [T],
    next: Option<&'a mut Option<Box<Bucket<T>>>>,
}
impl<'a, T: Default> Allocator<'a, T> {
    pub fn alloc_default(&mut self) -> &'a mut T {
        if self.remaining.is_empty() {
            let slot = self.next.take().expect("allocator link is always present");
            let bucket: &'a mut Bucket<T> = slot.insert(Box::new(Bucket {
                data: (0..FREAKY_BUCKET_SIZE).map(|_| T::default()).collect(),
                next: None,
            }));
            let Bucket { data, next } = bucket;
            self.remaining = data.as_mut_slice();
            self.next = Some(next);
        }
        let remaining = std::mem::take(&mut self.remaining);
        let (first, rest) = remaining
            .split_first_mut()
            .expect("a fresh bucket is never empty");
        self.remaining = rest;
        first
    }
}
