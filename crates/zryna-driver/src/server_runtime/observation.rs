//! Actual owned-store/resource destruction observations for the internal acceptance harness.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[derive(Default)]
pub(super) struct Observation {
    pub(super) stores: AtomicUsize,
    pub(super) stores_created: AtomicUsize,
    pub(super) stores_destroyed: AtomicUsize,
    pub(super) resources: AtomicUsize,
    pub(super) created: AtomicUsize,
    pub(super) destroyed: AtomicUsize,
    pub(super) clock_reads: AtomicUsize,
    pub(super) denials: AtomicUsize,
    pub(super) engines_constructed: AtomicUsize,
    pub(super) incoming_bytes: AtomicUsize,
    pub(super) remaining_fuel: AtomicU64,
    pub(super) continued_epochs: AtomicUsize,
    pub(super) returned_calls: AtomicUsize,
    pub(super) input_copies: AtomicUsize,
    pub(super) input_copy_bytes: AtomicUsize,
    pub(super) revocations: AtomicUsize,
}

impl Observation {
    pub(super) fn live(&self) -> (usize, usize) {
        (self.stores.load(Ordering::SeqCst), self.resources.load(Ordering::SeqCst))
    }
}
