//! Bounded server request leases, revocation and joined owned-resource retirement.
//! The authenticated server composition supplies component, grant and transport authority.

mod limits;
mod registry;
mod request;

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

pub(crate) use limits::{Error, Input, Limits};
use registry::{Entry, Shared, State};
pub(crate) use request::Request;

/// One host lifecycle with an owned and joined deadline worker; not a component host.
pub(crate) struct Server {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<Result<(), Error>>>,
}

impl Server {
    /// Validate limits before allocating host state or starting a worker.
    pub(crate) fn start(limits: Limits) -> Result<Self, Error> {
        let shared = Arc::new(Shared {
            limits: limits.validate()?,
            state: Mutex::new(State {
                stopped: false,
                next: 0,
                bytes: 0,
                live: 0,
                failed: false,
                entries: BTreeMap::new(),
            }),
            wake: std::sync::Condvar::new(),
            #[cfg(test)]
            stop_hook: Mutex::new(None),
        });
        let watched = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("zryna-server-deadline".to_owned())
            .spawn(move || {
                let result = watched.watch();
                if result.is_err() {
                    watched.stop();
                }
                result
            })
            .map_err(|_| Error::Host)?;
        Ok(Self { shared, worker: Some(worker) })
    }

    /// Admit one bounded already-decoded request; rejection retains no new input or authority.
    pub(crate) fn admit(&self, input: &Input<'_>) -> Result<Request, Error> {
        let reservation = input.reservation(self.shared.limits)?;
        let mut state = self.shared.lock()?;
        state.active()?;
        if input.deadline <= std::time::Instant::now() {
            return Err(Error::Deadline);
        }
        let total = state.bytes.checked_add(reservation).ok_or(Error::Limit)?;
        if state.live >= self.shared.limits.requests || total > self.shared.limits.buffer_bytes {
            return Err(Error::Limit);
        }
        let id = state.next;
        let next = id.checked_add(1).ok_or(Error::Limit)?;
        // Metadata, body and maximum response storage are charged before materialization.
        let entry = Entry {
            deadline: input.deadline,
            reservation,
            _method: input.method.to_owned(),
            _path: input.path.to_owned(),
            _body: input.body.to_vec(),
            authority: None,
        };
        state.entries.insert(id, entry);
        state.next = next;
        state.bytes = total;
        state.live += 1;
        drop(state);
        self.shared.wake.notify_all();
        Ok(Request { shared: Arc::clone(&self.shared), id })
    }

    /// Observe live reservations; this is not a teardown certificate for a guest store.
    #[cfg(test)]
    pub(crate) fn usage(&self) -> Result<(usize, usize), Error> {
        let state = self.shared.lock()?;
        Ok((state.live, state.bytes))
    }

    /// Revoke all requests, destroy retained resources, and join the deadline worker.
    pub(crate) fn shutdown(mut self) -> Result<(), Error> {
        self.terminate()
    }

    fn terminate(&mut self) -> Result<(), Error> {
        self.shared.stop();
        let joined =
            self.worker.take().map_or(Ok(()), |worker| worker.join().map_err(|_| Error::Host)?);
        let cleaned = self.shared.await_empty();
        joined.and(cleaned)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}
