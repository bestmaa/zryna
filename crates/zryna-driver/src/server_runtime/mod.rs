//! Private executable server acceptance slice. No listener or public driver selector.

mod envelope;
mod execution;
mod grants;
mod host;
mod linker;
mod observation;
mod operations;
mod prepared;
mod resources;
#[cfg(test)]
mod startup;

#[cfg(test)]
mod tests;

use std::{fmt, sync::Arc};

use envelope::Envelope;
use execution::Pending;
use grants::Approval;
use observation::Observation;
use prepared::Prepared;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Error {
    Grant,
    Limit,
    Artifact,
    Resource,
    Denied,
    Inactive,
    Deadline,
    Host,
    Guest,
    Lifecycle(crate::server_lifecycle::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "private server failed: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<crate::server_lifecycle::Error> for Error {
    fn from(error: crate::server_lifecycle::Error) -> Self {
        Self::Lifecycle(error)
    }
}

struct Server {
    lifecycle: Option<crate::server_lifecycle::Server>,
    prepared: Arc<Prepared>,
}

impl Server {
    pub(super) fn start(prepared: Prepared) -> Result<Self, Error> {
        prepared.revalidate()?;
        let lifecycle = crate::server_lifecycle::Server::start(prepared.envelope.requests)?;
        Ok(Self { lifecycle: Some(lifecycle), prepared: Arc::new(prepared) })
    }

    pub(super) fn admit(
        &self,
        input: &crate::server_lifecycle::Input<'_>,
    ) -> Result<Pending, Error> {
        let request = self.lifecycle.as_ref().ok_or(Error::Inactive)?.admit(input)?;
        execution::start(request, input, &self.prepared)
    }

    pub(super) fn usage(&self) -> Result<(usize, usize), Error> {
        Ok(self.lifecycle.as_ref().ok_or(Error::Inactive)?.usage()?)
    }

    pub(super) fn shutdown(mut self) -> Result<(), Error> {
        self.terminate()
    }

    fn terminate(&mut self) -> Result<(), Error> {
        if let Some(lifecycle) = self.lifecycle.take() {
            lifecycle.shutdown()?;
        }
        if self.prepared.observation.live() != (0, 0) {
            return Err(Error::Host);
        }
        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}
