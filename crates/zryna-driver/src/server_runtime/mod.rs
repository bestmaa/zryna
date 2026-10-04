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

pub(crate) use envelope::Envelope;
pub(crate) use execution::Pending;
pub(crate) use grants::Approval;
pub(crate) use observation::Observation;
pub(crate) use prepared::{Preparation, Prepared};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Error {
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

pub(crate) struct Server {
    lifecycle: Option<crate::server_lifecycle::Server>,
    prepared: Arc<Prepared>,
}

impl Server {
    pub(crate) fn start(prepared: Prepared) -> Result<Self, Error> {
        prepared.revalidate()?;
        let lifecycle = crate::server_lifecycle::Server::start(prepared.envelope.requests)?;
        Ok(Self { lifecycle: Some(lifecycle), prepared: Arc::new(prepared) })
    }

    pub(crate) fn admit(
        &self,
        input: &crate::server_lifecycle::Input<'_>,
    ) -> Result<Pending, Error> {
        let request = self.lifecycle.as_ref().ok_or(Error::Inactive)?.admit(input)?;
        execution::start(request, input, &self.prepared)
    }

    pub(crate) fn usage(&self) -> Result<(usize, usize), Error> {
        Ok(self.lifecycle.as_ref().ok_or(Error::Inactive)?.usage()?)
    }

    pub(crate) fn shutdown(mut self) -> Result<(), Error> {
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
