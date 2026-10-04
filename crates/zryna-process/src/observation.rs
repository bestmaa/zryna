//! Read-only pending-spawn observations, enabled only for deterministic test callers.

use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
    thread::ThreadId,
};

fn pending() -> &'static Mutex<HashSet<ThreadId>> {
    static PENDING: OnceLock<Mutex<HashSet<ThreadId>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(super) struct Pending(ThreadId);

impl Pending {
    pub(super) fn new() -> Self {
        let thread = std::thread::current().id();
        pending().lock().expect("pending-spawn observation lock").insert(thread);
        Self(thread)
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        pending().lock().expect("pending-spawn observation lock").remove(&self.0);
    }
}

pub(super) fn contains(thread: ThreadId) -> bool {
    pending().lock().expect("pending-spawn observation lock").contains(&thread)
}
