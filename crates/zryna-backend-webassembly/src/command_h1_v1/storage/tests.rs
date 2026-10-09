use std::sync::OnceLock;
use wasmtime::{
    Config, Engine, Instance, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc,
};

use super::{CANONICAL_START, MEMORY_END};

mod mutations;
mod resources;

type Realloc = TypedFunc<(i32, i32, i32, i32), i32>;

struct Storage {
    store: Store<StoreLimits>,
    instance: Instance,
    realloc: Realloc,
    memory: Memory,
}

impl Storage {
    fn new() -> Self {
        static ENGINE: OnceLock<Engine> = OnceLock::new();
        let engine = ENGINE.get_or_init(|| {
            let mut config = Config::new();
            config.consume_fuel(true).max_wasm_stack(65_536);
            Engine::new(&config).expect("pinned engine configuration")
        });
        let bytes = super::encode();
        super::audit::audit(&bytes).expect("independent storage topology and helper roles");
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
            .validate_all(&bytes)
            .expect("storage is WebAssembly 1.0");
        let module = Module::new(engine, bytes).expect("pinned engine accepts storage");
        let limits = StoreLimitsBuilder::new()
            .memory_size(usize::try_from(MEMORY_END).expect("memory bound"))
            .instances(1)
            .memories(1)
            .tables(0)
            .build();
        let mut store = Store::new(engine, limits);
        store.limiter(|limits| limits);
        store.set_fuel(50_000_000).expect("bounded test fuel");
        let instance = Instance::new(&mut store, &module, &[]).expect("fresh storage instance");
        let realloc = instance.get_typed_func(&mut store, "realloc").expect("closed realloc ABI");
        let memory = instance.get_memory(&mut store, "memory").expect("sole storage memory");
        assert_eq!(memory.size(&store), 256);
        assert!(memory.grow(&mut store, 1).is_err());
        Self { store, instance, realloc, memory }
    }

    fn allocate(&mut self, size: i32, align: i32) -> i32 {
        self.realloc.call(&mut self.store, (0, 0, align, size)).expect("bounded allocation")
    }

    fn free(&mut self, pointer: i32, size: i32, align: i32) {
        assert_eq!(
            self.realloc.call(&mut self.store, (pointer, size, align, 0)).expect("exact release"),
            0
        );
    }

    fn state(&mut self, selector: i32) -> i32 {
        self.instance
            .get_typed_func::<i32, i32>(&mut self.store, "canonical-state")
            .expect("closed observation ABI")
            .call(&mut self.store, selector)
            .expect("host observation")
    }

    fn drain(&mut self) -> wasmtime::Result<()> {
        self.instance
            .get_typed_func::<(), ()>(&mut self.store, "drain")
            .expect("closed drain ABI")
            .call(&mut self.store, ())
    }

    fn assert_fatal(&mut self, arguments: (i32, i32, i32, i32), reason: i32) {
        assert!(self.realloc.call(&mut self.store, arguments).is_err());
        assert_eq!(self.state(3), reason);
        assert!(self.realloc.call(&mut self.store, (0, 0, 1, 1)).is_err());
        assert!(self.drain().is_err());
        let validate = self
            .instance
            .get_typed_func::<(i32, i32, i32), i32>(&mut self.store, "validate")
            .expect("closed validation ABI");
        assert!(validate.call(&mut self.store, (0, 0, 1)).is_err());
    }
}

#[test]
fn command_storage_resize_preserves_bytes_and_releases_old_entry() {
    let mut storage = Storage::new();
    let first = storage.allocate(7, 1);
    assert_eq!(first, CANONICAL_START);
    storage
        .memory
        .write(&mut storage.store, usize::try_from(first).expect("pointer"), b"abcdefg")
        .expect("canonical test bytes");
    let grown = storage.realloc.call(&mut storage.store, (first, 7, 1, 12)).expect("grow");
    assert_eq!(grown, first + 7);
    assert_eq!(storage.state(1), 1);
    assert_eq!(storage.state(2), 2);
    let mut bytes = [0; 7];
    storage
        .memory
        .read(&storage.store, usize::try_from(grown).expect("pointer"), &mut bytes)
        .expect("read grown allocation");
    assert_eq!(&bytes, b"abcdefg");
    let shrunk = storage.realloc.call(&mut storage.store, (grown, 12, 1, 3)).expect("shrink");
    let mut prefix = [0; 3];
    storage
        .memory
        .read(&storage.store, usize::try_from(shrunk).expect("pointer"), &mut prefix)
        .expect("read shrunk allocation");
    assert_eq!(&prefix, b"abc");
    storage.free(shrunk, 3, 1);
    assert_eq!(storage.state(1), 0);
    assert_eq!(storage.state(0), CANONICAL_START);
    assert_eq!(storage.state(2), 3);
}

#[test]
fn command_storage_zero_alignment_and_partial_drain_are_exact() {
    let mut storage = Storage::new();
    assert_eq!(storage.allocate(0, 1), 0);
    assert_eq!(storage.state(2), 0);
    let first = storage.allocate(1, 1);
    let aligned = storage.allocate(8, 4);
    assert_eq!(aligned, CANONICAL_START + 4);
    storage.free(first, 1, 1);
    assert_eq!(storage.state(0), aligned + 8);
    assert_eq!(storage.state(1), 1);
    storage.drain().expect("drain remaining exact ledger");
    assert_eq!(storage.state(0), CANONICAL_START);
    assert_eq!(storage.state(1), 0);
    assert_eq!(storage.state(2), 2);
    assert_eq!(storage.allocate(4, 4), CANONICAL_START);
}

#[test]
fn command_storage_rejects_independent_foreign_old_lengths_alignment_and_double_free() {
    for arguments in [
        (0, 1, 1, 4),
        (0, 0, 0, 4),
        (0, 0, 2, 4),
        (0, 0, 8, 4),
        (0, 0, -1, 4),
        (1, 0, 1, 4),
        (65_536, 4, 1, 4),
        (-4, 4, 4, 4),
    ] {
        Storage::new().assert_fatal(arguments, 1);
    }
    for (offset, length, align) in [(0, 3, 4), (0, 5, 4), (1, 4, 4), (0, 4, 1), (0, -1, 4)] {
        let mut storage = Storage::new();
        let pointer = storage.allocate(4, 4);
        storage.assert_fatal((pointer + offset, length, align, 0), 1);
        assert_eq!(storage.state(1), 1);
    }
    let mut storage = Storage::new();
    let pointer = storage.allocate(4, 4);
    storage.free(pointer, 4, 4);
    storage.assert_fatal((pointer, 4, 4, 0), 1);
}

#[test]
fn command_storage_validation_requires_live_base_and_allocation_extent() {
    let mut storage = Storage::new();
    let pointer = storage.allocate(16, 4);
    let validate = storage
        .instance
        .get_typed_func::<(i32, i32, i32), i32>(&mut storage.store, "validate")
        .expect("closed validation ABI");
    assert_eq!(validate.call(&mut storage.store, (pointer, 8, 4)).expect("logical subextent"), 16);
    assert_eq!(validate.call(&mut storage.store, (pointer, 16, 4)).expect("exact extent"), 16);
    assert_eq!(validate.call(&mut storage.store, (0, 0, 1)).expect("empty string"), 0);
    assert!(validate.call(&mut storage.store, (pointer + 4, 4, 4)).is_err());
    assert_eq!(storage.state(3), 1);
    assert!(storage.drain().is_err());
    for arguments in [(16, 17, 4), (16, -1, 4), (16, 8, 1), (0, 1, 1)] {
        let mut storage = Storage::new();
        let pointer = storage.allocate(16, 4);
        let validate = storage
            .instance
            .get_typed_func::<(i32, i32, i32), i32>(&mut storage.store, "validate")
            .expect("closed validation ABI");
        let base = if arguments.0 == 0 { 0 } else { pointer };
        assert!(validate.call(&mut storage.store, (base, arguments.1, arguments.2)).is_err());
        assert_eq!(storage.state(3), 1);
    }
}

#[test]
fn command_storage_language_failure_retains_canonical_drain() {
    let mut storage = Storage::new();
    storage.allocate(8, 4);
    let allocate = storage
        .instance
        .get_typed_func::<i32, i32>(&mut storage.store, "allocate")
        .expect("closed language allocation ABI");
    assert_eq!(
        allocate
            .call(&mut storage.store, CANONICAL_START - super::LANGUAGE_START)
            .expect("exact language arena"),
        super::LANGUAGE_START
    );
    assert_eq!(allocate.call(&mut storage.store, 1).expect("checked allocation failure"), 0);
    let status =
        storage.instance.get_global(&mut storage.store, "status").expect("language status");
    assert_eq!(status.get(&mut storage.store).i32(), Some(2));
    assert_eq!(storage.state(3), 0);
    storage.drain().expect("language failure permits canonical cleanup");
    assert_eq!(storage.state(1), 0);
}
