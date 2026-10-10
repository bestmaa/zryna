//! Actual owned-store/resource destruction observations for the internal acceptance harness.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[derive(Default)]
pub(crate) struct Observation {
    pub(crate) stores: AtomicUsize,
    pub(crate) stores_created: AtomicUsize,
    pub(crate) stores_destroyed: AtomicUsize,
    pub(crate) resources: AtomicUsize,
    pub(crate) created: AtomicUsize,
    pub(crate) destroyed: AtomicUsize,
    pub(crate) clock_reads: AtomicUsize,
    pub(crate) denials: AtomicUsize,
    pub(crate) engines_constructed: AtomicUsize,
    pub(crate) incoming_bytes: AtomicUsize,
    pub(crate) remaining_fuel: AtomicU64,
    pub(crate) continued_epochs: AtomicUsize,
    pub(crate) returned_calls: AtomicUsize,
    pub(crate) input_copies: AtomicUsize,
    pub(crate) input_copy_bytes: AtomicUsize,
    pub(crate) revocations: AtomicUsize,
}

impl Observation {
    pub(crate) fn live(&self) -> (usize, usize) {
        (self.stores.load(Ordering::SeqCst), self.resources.load(Ordering::SeqCst))
    }
}
