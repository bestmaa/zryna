//! Private loopback implementation and process corpus; no public selector or grant activation.

mod cli;
mod config;
mod framing;
mod io;
mod listener;
#[cfg(test)]
mod process_fixture;
#[cfg(test)]
mod process_tests;
pub(super) mod source;
#[cfg(test)]
mod tests;

use config::Config;
use listener::Bound;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Error {
    Config,
    Framing,
    Limit,
    Deadline,
    Stopped,
    Io,
    Runtime(crate::server_runtime::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "private loopback server failed: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<crate::server_runtime::Error> for Error {
    fn from(error: crate::server_runtime::Error) -> Self {
        Self::Runtime(error)
    }
}
