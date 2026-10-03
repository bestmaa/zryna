//! Non-cloneable request owner and revocation-only handles.

use std::{sync::Arc, time::Instant};

use super::{
    Error,
    registry::{Entry, Shared},
};

/// A lease into one registry; stale handles cannot select a replacement request.
pub(crate) struct Request {
    pub(super) shared: Arc<Shared>,
    pub(super) id: u64,
}

/// Clones only cancellation access, never buffers or retained authority.
#[derive(Clone)]
pub(crate) struct Cancellation {
    shared: Arc<Shared>,
    id: u64,
}

impl Cancellation {
    /// Idempotently revoke and destroy this request's retained resources.
    pub(crate) fn cancel(&self) -> Result<(), Error> {
        self.shared.remove(self.id)
    }
}

impl Request {
    pub(crate) fn cancellation(&self) -> Cancellation {
        Cancellation { shared: Arc::clone(&self.shared), id: self.id }
    }

    /// Check activity at every future adapter callback and before publication.
    pub(crate) fn check(&self) -> Result<(), Error> {
        let mut state = self.shared.lock()?;
        state.active()?;
        let entry = state.entries.get(&self.id).ok_or(Error::Inactive)?;
        if entry.deadline <= Instant::now() {
            let removed = state.remove(self.id);
            drop(state);
            if let Some(entry) = removed {
                self.shared.retire(entry)?;
            }
            return Err(Error::Deadline);
        }
        Ok(())
    }

    /// Copy into caller-provided bounded storage; retain no references to registry buffers.
    pub(crate) fn copy_input(&self, output: &mut [u8]) -> Result<usize, Error> {
        self.check()?;
        let state = self.shared.lock()?;
        state.active()?;
        let entry = state.entries.get(&self.id).ok_or(Error::Inactive)?;
        if entry.deadline <= Instant::now() {
            return Err(Error::Deadline);
        }
        if output.len() < entry.body.len() {
            return Err(Error::Limit);
        }
        output[..entry.body.len()].copy_from_slice(&entry.body);
        Ok(entry.body.len())
    }

    /// Transfer one trusted adapter resource. This API does not verify or grant that resource.
    pub(crate) fn retain<T: Send + 'static>(&self, authority: T) -> Result<(), Error> {
        self.check()?;
        let mut state = self.shared.lock()?;
        state.active()?;
        let entry = state.entries.get_mut(&self.id).ok_or(Error::Inactive)?;
        if entry.deadline <= Instant::now() {
            return Err(Error::Deadline);
        }
        if entry.authority.is_some() {
            return Err(Error::Limit);
        }
        entry.authority = Some(Box::new(authority));
        Ok(())
    }

    /// Capability callbacks fail closed and revoke the whole request.
    pub(crate) fn deny_capability(&self) -> Result<(), Error> {
        self.shared.remove(self.id)?;
        Err(Error::Denied)
    }

    /// Consuming response publication, linearized against cancellation and host termination.
    /// An invalid or oversized response also consumes and cleans up the request.
    pub(crate) fn finish(self, status: u16, body: &[u8]) -> Result<Vec<u8>, Error> {
        let (entry, response) = {
            let mut state = self.shared.lock()?;
            state.active()?;
            let entry = state.remove(self.id).ok_or(Error::Inactive)?;
            let response = if entry.deadline <= Instant::now() {
                Err(Error::Deadline)
            } else if !(200..=599).contains(&status) {
                Err(Error::Malformed)
            } else if body.len() > self.shared.limits.response_bytes {
                Err(Error::Limit)
            } else {
                Ok(body.to_vec())
            };
            (entry, response)
        };
        // Guest/adapter authority must be destroyed before a response escapes.
        let deadline = entry.deadline;
        self.shared.retire(entry)?;
        self.shared.wake.notify_all();
        if response.is_ok() {
            // Cleanup may reenter or overlap termination. Publication is ordered by this
            // final state lock, after authority destruction, rather than response preparation.
            let state = self.shared.lock()?;
            state.active()?;
            if deadline <= Instant::now() {
                return Err(Error::Deadline);
            }
        }
        response
    }

    pub(crate) fn route(&self) -> Result<(String, String), Error> {
        self.check()?;
        let state = self.shared.lock()?;
        state.active()?;
        let Entry { method, path, deadline, .. } =
            state.entries.get(&self.id).ok_or(Error::Inactive)?;
        if *deadline <= Instant::now() {
            return Err(Error::Deadline);
        }
        Ok((method.clone(), path.clone()))
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        let _ = self.shared.remove(self.id);
    }
}
