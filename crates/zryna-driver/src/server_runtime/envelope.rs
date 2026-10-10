//! Independent guest-store limits; compilation and JIT memory are not guest linear memory.

use std::time::Duration;

use wasmtime::{Config, Engine, StoreLimits, StoreLimitsBuilder, WasmFeatures};

use super::Error;
use crate::server_lifecycle::Limits;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Envelope {
    pub(crate) requests: Limits,
    pub(crate) memory_bytes: usize,
    pub(crate) fuel: u64,
    pub(crate) resources: usize,
    pub(crate) callbacks: u32,
}

impl Envelope {
    pub(super) fn validate(self) -> Result<Self, Error> {
        // The second half reserves the execution-owned copy of incoming bytes/metadata.
        if self.requests.requests == 0
            || self.requests.requests > 64
            || self.requests.request_bytes == 0
            || self.requests.request_bytes > 65_536
            || self.requests.response_bytes == 0
            || self.requests.response_bytes > 65_536
            || self.requests.buffer_bytes == 0
            || self.requests.buffer_bytes > 4 * 1024 * 1024
            || self.requests.timeout.is_zero()
            || self.requests.timeout > Duration::from_secs(5)
            || self.memory_bytes == 0
            || self.memory_bytes > 65_536
            || self.fuel == 0
            || self.fuel > 100_000
            || self.resources == 0
            || self.resources > 8
            || self.callbacks == 0
            || self.callbacks > 16
        {
            return Err(Error::Limit);
        }
        Ok(self)
    }

    pub(super) fn store_limits(self) -> StoreLimits {
        StoreLimitsBuilder::new()
            .instances(3)
            .memories(1)
            .tables(0)
            .memory_size(self.memory_bytes)
            .table_elements(0)
            .trap_on_grow_failure(true)
            .build()
    }

    pub(crate) fn identity(self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in [
            self.requests.requests as u64,
            self.requests.request_bytes as u64,
            self.requests.response_bytes as u64,
            self.requests.buffer_bytes as u64,
            self.memory_bytes as u64,
            self.fuel,
            self.resources as u64,
            u64::from(self.callbacks),
        ] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(self.requests.timeout.as_nanos().to_le_bytes());
        bytes
    }
}

pub(super) fn engine() -> Result<Engine, Error> {
    let mut config = Config::new();
    let features =
        WasmFeatures::WASM1.difference(WasmFeatures::GC_TYPES).union(WasmFeatures::COMPONENT_MODEL);
    config
        .wasm_features(WasmFeatures::all(), false)
        .wasm_features(features, true)
        .consume_fuel(true)
        .epoch_interruption(true)
        .max_wasm_stack(64 * 1024)
        .wasm_backtrace_max_frames(None);
    Engine::new(&config).map_err(|_| Error::Host)
}
