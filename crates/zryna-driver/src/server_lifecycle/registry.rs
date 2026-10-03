//! One mutex linearizes publication, revocation and reservation reuse.

use std::{
    collections::BTreeMap,
    sync::{Condvar, Mutex, MutexGuard},
    time::Instant,
};

use super::{Error, Limits};

#[cfg(test)]
#[path = "termination_tests.rs"]
mod termination_tests;

#[cfg(test)]
type StopHook = Box<dyn FnOnce() + Send>;

pub(super) struct Entry {
    pub(super) deadline: Instant,
    pub(super) reservation: usize,
    pub(super) method: String,
    pub(super) path: String,
    pub(super) body: Vec<u8>,
    // Future adapters transfer ownership here; lifecycle handles cannot clone it back out.
    pub(super) authority: Option<Box<dyn Send>>,
}

pub(super) struct State {
    pub(super) stopped: bool,
    pub(super) next: u64,
    pub(super) bytes: usize,
    pub(super) live: usize,
    pub(super) failed: bool,
    pub(super) entries: BTreeMap<u64, Entry>,
}

impl State {
    pub(super) fn active(&self) -> Result<(), Error> {
        if self.stopped { Err(Error::Inactive) } else { Ok(()) }
    }

    fn stop(&mut self) -> BTreeMap<u64, Entry> {
        self.stopped = true;
        std::mem::take(&mut self.entries)
    }

    pub(super) fn remove(&mut self, id: u64) -> Option<Entry> {
        self.entries.remove(&id)
    }
}

pub(super) struct Shared {
    pub(super) limits: Limits,
    pub(super) state: Mutex<State>,
    pub(super) wake: Condvar,
    #[cfg(test)]
    pub(super) stop_hook: Mutex<Option<StopHook>>,
}

impl Shared {
    pub(super) fn lock(&self) -> Result<MutexGuard<'_, State>, Error> {
        self.state.lock().map_err(|_| Error::Host)
    }

    pub(super) fn remove(&self, id: u64) -> Result<(), Error> {
        let entry = {
            let mut state = self.lock()?;
            state.remove(id)
        };
        // Drop retained authority outside the registry lock; destructors may reenter the host.
        if let Some(entry) = entry {
            self.retire(entry)?;
        }
        self.wake.notify_all();
        Ok(())
    }

    pub(super) fn retire(&self, entry: Entry) -> Result<(), Error> {
        let bytes = entry.reservation;
        let destroyed =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(entry))).is_ok();
        // A slot is reusable only after its resource and buffers have actually been destroyed.
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.bytes -= bytes;
        state.live -= 1;
        let revoked = if destroyed {
            BTreeMap::new()
        } else {
            state.failed = true;
            // Stopping and detaching siblings share one transition; no callback can slip
            // between a stopped flag and a later acquisition of the revocation lock.
            state.stop()
        };
        drop(state);
        self.wake.notify_all();
        if destroyed {
            Ok(())
        } else {
            self.retire_all(revoked);
            Err(Error::Host)
        }
    }

    pub(super) fn await_empty(&self) -> Result<(), Error> {
        let mut state = self.lock()?;
        while state.live != 0 {
            state = self.wake.wait(state).map_err(|_| Error::Host)?;
        }
        if state.failed { Err(Error::Host) } else { Ok(()) }
    }

    pub(super) fn stop(&self) {
        // Poison cannot preserve authority during termination.
        let entries = {
            let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            state.stop()
        };
        self.wake.notify_all();
        self.retire_all(entries);
    }

    fn retire_all(&self, entries: BTreeMap<u64, Entry>) {
        #[cfg(test)]
        self.before_stop_cleanup();
        for entry in entries.into_values() {
            let _ = self.retire(entry);
        }
    }

    #[cfg(test)]
    fn before_stop_cleanup(&self) {
        let hook = { self.stop_hook.lock().expect("test hook mutex").take() };
        if let Some(hook) = hook {
            hook();
        }
    }

    pub(super) fn watch(&self) -> Result<(), Error> {
        let mut state = self.lock()?;
        loop {
            if state.stopped {
                return Ok(());
            }
            let now = Instant::now();
            let expired: Vec<_> = state
                .entries
                .iter()
                .filter_map(|(id, entry)| (entry.deadline <= now).then_some(*id))
                .collect();
            if !expired.is_empty() {
                let removed: Vec<_> =
                    expired.into_iter().filter_map(|id| state.remove(id)).collect();
                drop(state);
                let mut failed = false;
                for entry in removed {
                    failed |= self.retire(entry).is_err();
                }
                if failed {
                    return Err(Error::Host);
                }
                state = self.lock()?;
                continue;
            }
            state = if let Some(deadline) = state.entries.values().map(|entry| entry.deadline).min()
            {
                self.wake
                    .wait_timeout(state, deadline.saturating_duration_since(now))
                    .map_err(|_| Error::Host)?
                    .0
            } else {
                self.wake.wait(state).map_err(|_| Error::Host)?
            };
        }
    }
}
