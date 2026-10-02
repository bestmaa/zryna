//! One fresh engine, finite execution budgets and an explicitly joined deadline worker.

use std::{num::NonZeroUsize, sync::mpsc, thread::JoinHandle, time::Duration};
use wasmtime::{Config, Engine, StoreLimits, StoreLimitsBuilder, WasmFeatures};

pub(super) const FUEL: u64 = 100_000;
pub(super) const DEADLINE: Duration = Duration::from_secs(5);

pub(super) fn engine() -> wasmtime::Result<Engine> {
    let mut config = Config::new();
    let features =
        WasmFeatures::WASM1.difference(WasmFeatures::GC_TYPES).union(WasmFeatures::COMPONENT_MODEL);
    config
        .wasm_features(WasmFeatures::all(), false)
        .wasm_features(features, true)
        .consume_fuel(true)
        .epoch_interruption(true)
        .generate_address_map(true)
        .max_wasm_stack(65_536)
        .wasm_backtrace_max_frames(NonZeroUsize::new(1));
    Engine::new(&config)
}

pub(super) fn limits() -> StoreLimits {
    StoreLimitsBuilder::new()
        .instances(2)
        .memories(1)
        .tables(0)
        .memory_size(16_777_216)
        .table_elements(0)
        .trap_on_grow_failure(true)
        .build()
}

pub(super) struct Deadline {
    stop: mpsc::SyncSender<()>,
    worker: Option<JoinHandle<()>>,
}

impl Deadline {
    pub(super) fn start(engine: &Engine) -> std::io::Result<Self> {
        let (stop, receiver) = mpsc::sync_channel(1);
        let engine = engine.clone();
        let worker =
            std::thread::Builder::new().name("command-deadline".into()).spawn(move || {
                if matches!(receiver.recv_timeout(DEADLINE), Err(mpsc::RecvTimeoutError::Timeout)) {
                    engine.increment_epoch();
                }
            })?;
        Ok(Self { stop, worker: Some(worker) })
    }

    pub(super) fn finish(mut self) -> bool {
        self.join()
    }
    fn join(&mut self) -> bool {
        let _ = self.stop.try_send(());
        self.worker.take().is_none_or(|worker| worker.join().is_ok())
    }
}

impl Drop for Deadline {
    fn drop(&mut self) {
        let _ = self.join();
    }
}
