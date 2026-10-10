//! Bounded loopback transport; authenticated composition owns public source and listener approval.

#[cfg(test)]
mod cli;
mod config;
mod framing;
mod io;
mod listener;
#[cfg(test)]
mod process_fixture;
#[cfg(test)]
mod process_tests;
#[cfg(test)]
pub(super) mod source;
#[cfg(test)]
mod tests;

pub(crate) use config::Config;
pub(crate) type Control = io::Control;
pub(crate) type Observation = io::Observation;
pub(crate) use listener::Bound;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Completion {
    Attempts,
    Deadline,
    Cancelled,
}
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Error {
    Config,
    Framing,
    Limit,
    Deadline,
    Stopped,
    Io,
    Authority,
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
