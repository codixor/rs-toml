use super::*;

#[test]
fn default_impl() {
    // Arena::default() should work like Arena::new()
    let arena = Arena::default();
    let ptr = arena.alloc(16);
    assert_eq!(ptr.as_ptr() as usize % 8, 0);
    unsafe { std::ptr::write_bytes(ptr.as_ptr(), 0xAA, 16) };
    let bytes = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), 16) };
    assert!(bytes.iter().all(|&b| b == 0xAA));
}

#[test]
fn alloc_basics() {
    let arena = Arena::new();

    // Single allocation: write and read back.
    let ptr = arena.alloc(8);
    unsafe { ptr.as_ptr().write(0xAB) };
    assert_eq!(unsafe { *ptr.as_ptr() }, 0xAB);

    // All allocations are 8-byte aligned.
    for size in [8, 16, 24, 64] {
        let p = arena.alloc(size);
        assert_eq!(p.as_ptr() as usize % 8, 0, "size={size}");
    }

    // Multiple allocations do not overlap.
    let a = arena.alloc(64);
    let b = arena.alloc(64);
    let c = arena.alloc(64);

    let a_range = a.as_ptr() as usize..a.as_ptr() as usize + 64;
    let b_start = b.as_ptr() as usize;
    let c_start = c.as_ptr() as usize;

    assert!(!a_range.contains(&b_start));
    assert!(!a_range.contains(&c_start));
    assert_ne!(b_start, c_start);
}

#[test]
fn alloc_growth() {
    let arena = Arena::new();

    // Allocate well beyond INITIAL_SLAB_SIZE to force at least one slab growth.
    for _ in 0..20 {
        let ptr = arena.alloc(128);
        unsafe {
            std::ptr::write_bytes(ptr.as_ptr(), 0xCC, 128);
        }
    }

    // A single large allocation that exceeds the initial slab size.
    let ptr = arena.alloc(4096);
    unsafe {
        std::ptr::write_bytes(ptr.as_ptr(), 0xDD, 4096);
    }
    assert_eq!(ptr.as_ptr() as usize % 8, 0);
}

#[test]
fn realloc_in_place() {
    let arena = Arena::new();
    let mut ptr = arena.alloc(24);
    unsafe { std::ptr::write_bytes(ptr.as_ptr(), 0xAA, 24) };
    let original = ptr;

    // No intervening alloc -- should extend in place.
    let new_ptr = unsafe { arena.realloc(ptr, 24, 48) };
    assert_eq!(new_ptr, original, "expected in-place extension");

    // Original data preserved.
    let bytes = unsafe { std::slice::from_raw_parts(new_ptr.as_ptr(), 24) };
    assert!(bytes.iter().all(|&b| b == 0xAA));

    // Successive reallocs with no intervening allocs should all extend in place.
    ptr = new_ptr;
    for new_size in [96, 192] {
        let old_size = new_size / 2;
        ptr = unsafe { arena.realloc(ptr, old_size, new_size) };
        assert_eq!(
            ptr, original,
            "size={new_size}: expected in-place extension"
        );
    }

    // Original data still intact after multiple in-place grows.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), 24) };
    assert!(bytes.iter().all(|&b| b == 0xAA));
}

#[test]
fn realloc_intervening_alloc_forces_copy() {
    let arena = Arena::new();
    let a = arena.alloc(24);
    unsafe { std::ptr::write_bytes(a.as_ptr(), 0xBB, 24) };

    // Intervening allocation moves the head.
    let _b = arena.alloc(8);

    let new_ptr = unsafe { arena.realloc(a, 24, 48) };
    assert_ne!(new_ptr, a, "expected copy to new location");

    // Original data preserved in new location.
    let bytes = unsafe { std::slice::from_raw_parts(new_ptr.as_ptr(), 24) };
    assert!(bytes.iter().all(|&b| b == 0xBB));
}

#[test]
fn realloc_cross_slab() {
    let arena = Arena::new();
    // Fill the first slab almost completely.
    let ptr = arena.alloc(INITIAL_SLAB_SIZE - HEADER_SIZE);
    unsafe { std::ptr::write_bytes(ptr.as_ptr(), 0xCC, 64) };

    // Realloc to a size that cannot fit in the remaining slab space.
    let new_ptr = unsafe { arena.realloc(ptr, INITIAL_SLAB_SIZE - HEADER_SIZE, INITIAL_SLAB_SIZE) };
    assert_ne!(new_ptr, ptr, "expected new slab allocation");

    // First 64 bytes of data preserved.
    let bytes = unsafe { std::slice::from_raw_parts(new_ptr.as_ptr(), 64) };
    assert!(bytes.iter().all(|&b| b == 0xCC));
}

#[test]
#[allow(clippy::drop_non_drop)]
fn scratch_basics() {
    let arena = Arena::new();
    arena.alloc(8);

    // push builds content byte by byte.
    let mut scratch = arena.scratch();
    scratch.push(b'h');
    scratch.push(b'i');
    assert_eq!(scratch.as_str(), "hi");
    drop(scratch); // end borrow before creating next scratch

    // extend appends slices; length is tracked correctly.
    let mut scratch = arena.scratch();
    scratch.extend("hello ");
    scratch.extend("world");
    assert_eq!(scratch.as_str(), "hello world");
    assert_eq!(scratch.as_bytes().len(), 11);
    drop(scratch); // end borrow before creating next scratch

    // Empty scratch returns empty slice.
    let scratch = arena.scratch();
    assert_eq!(scratch.as_str(), "");
}

#[test]
fn scratch_commit() {
    let arena = Arena::new();
    arena.alloc(8);

    // Commit returns the written data as a slice.
    let mut scratch = arena.scratch();
    scratch.extend("committed");
    let slice = scratch.commit();
    assert_eq!(slice, "committed");

    // Committing advances the arena pointer so subsequent allocations don't overlap.
    let mut scratch = arena.scratch();
    scratch.extend("data");
    let committed = scratch.commit();
    let next = arena.alloc(8);
    let committed_range =
        committed.as_ptr() as usize..committed.as_ptr() as usize + committed.len();
    assert!(!committed_range.contains(&(next.as_ptr() as usize)));

    // Committing an empty scratch returns an empty slice.
    let scratch = arena.scratch();
    let slice = scratch.commit();
    assert_eq!(slice, "");
}

#[test]
fn scratch_drop_without_commit() {
    let arena = Arena::new();
    arena.alloc(8);
    let ptr_before = arena.ptr.get();

    // Dropping without commit leaves the arena pointer unchanged.
    {
        let mut scratch = arena.scratch();
        scratch.extend("discarded");
    }
    assert_eq!(arena.ptr.get(), ptr_before);

    // Allocating while a scratch is alive never overlaps the scratch bytes.
    let mut scratch = arena.scratch();
    scratch.extend("abc");
    let other = arena.alloc_str("xyz");
    scratch.extend("def");
    assert_eq!(scratch.commit(), "abcdef");
    assert_eq!(other, "xyz");

    // A new scratch can be created after a dropped one.
    let mut scratch = arena.scratch();
    scratch.extend("kept");
    let committed = scratch.commit();
    assert_eq!(committed, "kept");
}

#[test]
#[allow(clippy::drop_non_drop)]
fn scratch_growth() {
    let arena = Arena::new();
    arena.alloc(8);

    // extend with a large pattern preserves all data across slab overflow.
    let mut scratch = arena.scratch();
    let pattern: String = (b'a'..=b'z').cycle().take(2048).map(char::from).collect();
    scratch.extend(&pattern);
    assert_eq!(scratch.as_str(), pattern);
    drop(scratch); // end borrow before creating next scratch

    // push one byte at a time across growth boundaries preserves data.
    let mut scratch = arena.scratch();
    for i in 0..2048u16 {
        scratch.push((i & 0x7F) as u8);
    }
    assert_eq!(scratch.as_bytes().len(), 2048);
    for (i, &b) in scratch.as_bytes().iter().enumerate() {
        assert_eq!(b, (i & 0x7F) as u8, "mismatch at index {i}");
    }
    drop(scratch); // end borrow before creating next scratch

    // Multiple growth rounds via chunked extend.
    let mut scratch = arena.scratch();
    let mut expected = String::new();
    for round in 0u8..10 {
        let chunk: String = std::iter::repeat_n(char::from(b'a' + round), 512).collect();
        scratch.extend(&chunk);
        expected.push_str(&chunk);
    }
    assert_eq!(scratch.as_str(), expected);
    drop(scratch); // end borrow before creating next scratch

    // Commit after growth returns correct data and doesn't overlap next alloc.
    let mut scratch = arena.scratch();
    let data: String = (0..2048u32)
        .map(|i| char::from(b'!' + (i % 90) as u8))
        .collect();
    scratch.extend(&data);
    let committed = scratch.commit();
    assert_eq!(committed, data);

    let next = arena.alloc(64);
    let committed_end = committed.as_ptr() as usize + committed.len();
    assert!(next.as_ptr() as usize >= committed_end);
}

#[test]
fn alloc_then_scratch_then_alloc() {
    let arena = Arena::new();

    // First allocation.
    let a = arena.alloc(32);
    unsafe { std::ptr::write_bytes(a.as_ptr(), 0xAA, 32) };

    // Scratch in the middle.
    let mut scratch = arena.scratch();
    scratch.extend("middle");
    let mid = scratch.commit();
    assert_eq!(mid, "middle");

    // Second allocation.
    let b = arena.alloc(32);
    unsafe { std::ptr::write_bytes(b.as_ptr(), 0xBB, 32) };

    // Verify first allocation wasn't corrupted.
    let first_bytes = unsafe { std::slice::from_raw_parts(a.as_ptr(), 32) };
    assert!(first_bytes.iter().all(|&b| b == 0xAA));
}

#[test]
#[should_panic(expected = "layout overflow")]
fn alloc_usize_max_panics() {
    let arena = Arena::new();
    arena.alloc(8);
    arena.alloc(usize::MAX);
}

#[test]
#[should_panic(expected = "layout overflow")]
fn alloc_usize_max_unaligned_panics() {
    let arena = Arena::new();
    // Allocating 1 byte leaves the bump pointer 1 past an aligned boundary,
    // so the next alloc has padding = ALLOC_ALIGN - 1 = 7. With regular
    // addition, 7 + usize::MAX wraps to 6.
    arena.alloc(1);
    arena.alloc(usize::MAX);
}

#[test]
#[should_panic(expected = "layout overflow")]
fn alloc_overflow_smallest_wrapping_size() {
    let arena = Arena::new();
    arena.alloc(1); // bump pointer is now misaligned (padding will be 7)
    // Smallest size that wraps when padding is maximal:
    // 7 + (usize::MAX - 6) = usize::MAX + 1. Regular addition wraps to 0;
    // saturating_add correctly yields usize::MAX, failing the bounds check.
    arena.alloc(usize::MAX - (ALLOC_ALIGN - 2));
}

#[test]
fn alloc_overflow_preserves_arena_state() {
    let arena = Arena::new();
    arena.alloc(8);
    let ptr_before = arena.ptr.get();
    let end_before = arena.end.get();

    // grow() panics via checked_add before modifying any arena state, so
    // after catching the panic, ptr/end/slab must be unchanged.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        arena.alloc(usize::MAX);
    }));

    assert!(result.is_err());
    assert_eq!(arena.ptr.get(), ptr_before);
    assert_eq!(arena.end.get(), end_before);

    // Arena is still usable after the caught panic.
    let p = arena.alloc(16);
    assert_eq!(p.as_ptr() as usize % ALLOC_ALIGN, 0);
}

#[test]
#[should_panic(expected = "layout overflow")]
fn realloc_usize_max_panics() {
    let arena = Arena::new();
    let ptr = arena.alloc(8);
    // ptr is the most recent allocation (old_end == head), so realloc tries
    // the in-place path. The remaining-based bounds check must reject
    // usize::MAX without the old ptr + new_size addition wrapping.
    unsafe { arena.realloc(ptr, 8, usize::MAX) };
}
