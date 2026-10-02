use super::{CANONICAL_START, MEMORY_END, Storage};

#[test]
fn command_storage_live_limit_counts_old_and_prospective_resize() {
    let mut storage = Storage::new();
    let pointers = (0..16).map(|_| storage.allocate(4, 4)).collect::<Vec<_>>();
    assert_eq!(storage.state(1), 16);
    assert_eq!(storage.state(2), 16);
    storage.assert_fatal((0, 0, 4, 4), 2);
    assert_eq!(storage.state(1), 16);

    let mut storage = Storage::new();
    for _ in 0..16 {
        storage.allocate(4, 4);
    }
    storage.assert_fatal((pointers[0], 4, 4, 8), 2);
    assert_eq!(storage.state(1), 16);
    assert_eq!(storage.state(2), 16);

    let mut storage = Storage::new();
    let first = storage.allocate(4, 4);
    for _ in 1..15 {
        storage.allocate(4, 4);
    }
    assert!(storage.realloc.call(&mut storage.store, (first, 4, 4, 8)).is_ok());
    assert_eq!(storage.state(1), 15);
    assert_eq!(storage.state(2), 16);
}

#[test]
fn command_storage_allocation_count_survives_full_drain_and_rejects_first_extra() {
    let mut storage = Storage::new();
    for count in 1..=4096 {
        assert_eq!(storage.allocate(1, 1), CANONICAL_START);
        storage.drain().expect("complete drain");
        assert_eq!(storage.state(2), count);
    }
    storage.assert_fatal((0, 0, 1, 1), 2);
    assert_eq!(storage.state(2), 4096);
    assert_eq!(storage.state(1), 0);
    let mut fresh = Storage::new();
    assert_eq!(fresh.allocate(1, 1), CANONICAL_START);
    assert_eq!(fresh.state(2), 1);
}

#[test]
fn command_storage_allocation_byte_limit_and_wrapping_sizes_trap() {
    let mut storage = Storage::new();
    assert_eq!(storage.allocate(4096, 4), CANONICAL_START);
    storage.free(CANONICAL_START, 4096, 4);
    for size in [4097, i32::MAX, i32::MIN, -1] {
        let mut storage = Storage::new();
        storage.assert_fatal((0, 0, 1, size), 2);
        assert_eq!(storage.state(1), 0);
        assert_eq!(storage.state(2), 0);
    }
}

#[test]
fn command_storage_arena_counts_holes_and_exact_last_byte() {
    let mut storage = Storage::new();
    let anchor = storage.allocate(1, 1);
    for _ in 0..255 {
        let pointer = storage.allocate(4096, 1);
        storage.free(pointer, 4096, 1);
    }
    let tail = storage.allocate(4095, 1);
    assert_eq!(tail + 4095, MEMORY_END);
    assert_eq!(storage.state(0), MEMORY_END);
    assert_eq!(storage.state(1), 2);
    storage.assert_fatal((0, 0, 1, 1), 2);
    assert_eq!(storage.state(1), 2);
    assert_eq!(anchor, CANONICAL_START);

    let mut storage = Storage::new();
    let anchor = storage.allocate(1, 1);
    let next = storage.allocate(4096, 1);
    storage.free(next, 4096, 1);
    assert!(storage.state(0) > CANONICAL_START);
    storage.free(anchor, 1, 1);
    assert_eq!(storage.state(0), CANONICAL_START);
    assert_eq!(storage.allocate(4096, 4), CANONICAL_START);
}
