//! Closed internal request envelope. These limits do not grant WASI imports.

use std::time::{Duration, Instant};

/// Maximum concurrently retained incoming requests in this internal slice.
pub(crate) const MAX_REQUESTS: usize = 64;
/// Maximum bytes in either one incoming or one outgoing body.
pub(crate) const MAX_BODY_BYTES: usize = 65_536;
/// Maximum reserved body storage, independent of guest linear-memory limits.
pub(crate) const MAX_BUFFER_BYTES: usize = 8 * 1024 * 1024;
/// Maximum lifetime of an incoming request.
pub(crate) const MAX_DEADLINE: Duration = Duration::from_secs(5);

/// Explicit bounds, validated before any lifecycle worker is started.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub(crate) requests: usize,
    pub(crate) request_bytes: usize,
    pub(crate) response_bytes: usize,
    pub(crate) buffer_bytes: usize,
    pub(crate) timeout: Duration,
}

impl Limits {
    pub(super) fn validate(self) -> Result<Self, Error> {
        if self.requests == 0
            || self.requests > MAX_REQUESTS
            || self.request_bytes == 0
            || self.request_bytes > MAX_BODY_BYTES
            || self.response_bytes == 0
            || self.response_bytes > MAX_BODY_BYTES
            || self.buffer_bytes == 0
            || self.buffer_bytes > MAX_BUFFER_BYTES
            || self.timeout.is_zero()
            || self.timeout > MAX_DEADLINE
        {
            return Err(Error::Limit);
        }
        Ok(self)
    }
}

/// Stable internal rejection categories; no successful WIT return is fabricated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Error {
    /// A closed envelope or resource bound was exceeded.
    Limit,
    /// The minimal already-decoded HTTP request or response is invalid.
    Malformed,
    /// The supplied deadline has already expired.
    Deadline,
    /// The host is stopped, or the request was destroyed.
    Inactive,
    /// Every host capability is denied in this lifecycle-only slice.
    Denied,
    /// Lifecycle worker creation, locking, or joining failed.
    Host,
}

/// Borrowed input from a future audited incoming-handler adapter, not a raw HTTP parser.
pub(crate) struct Input<'a> {
    pub(crate) method: &'a str,
    pub(crate) path: &'a str,
    pub(crate) body: &'a [u8],
    pub(crate) deadline: Instant,
}

impl Input<'_> {
    pub(super) fn reservation(&self, limits: Limits) -> Result<usize, Error> {
        if !matches!(self.method, "GET" | "POST")
            || self.path.is_empty()
            || self.path.len() > 256
            || !self.path.starts_with('/')
            || self.path.bytes().any(|byte| !(0x21..=0x7e).contains(&byte) || byte == b'#')
        {
            return Err(Error::Malformed);
        }
        let now = Instant::now();
        if self.deadline <= now {
            return Err(Error::Deadline);
        }
        if self.deadline.duration_since(now) > limits.timeout
            || self.body.len() > limits.request_bytes
        {
            return Err(Error::Limit);
        }
        // Reserve the entire response before copying input, including retained metadata.
        self.body
            .len()
            .checked_add(self.path.len())
            .and_then(|bytes| bytes.checked_add(self.method.len()))
            .and_then(|bytes| bytes.checked_add(limits.response_bytes))
            .ok_or(Error::Limit)
    }
}
