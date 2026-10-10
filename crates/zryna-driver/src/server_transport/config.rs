//! Closed transport configuration, admitted before source compilation or listener creation.

use super::Error;
use crate::{server_lifecycle::Limits, server_runtime::Envelope};
use serde::Deserialize;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    listen: String,
    attempts: u16,
    header_bytes: usize,
    body_bytes: usize,
    request_ms: u64,
    service_ms: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Config {
    pub(crate) address: SocketAddr,
    pub(crate) attempts: u16,
    pub(crate) header_bytes: usize,
    pub(crate) body_bytes: usize,
    pub(crate) request_timeout: Duration,
    pub(crate) service_timeout: Duration,
}

impl Config {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.is_empty() || bytes.len() > 1024 {
            return Err(Error::Config);
        }
        let document: Document = serde_json::from_slice(bytes).map_err(|_| Error::Config)?;
        let address: SocketAddr = document.listen.parse().map_err(|_| Error::Config)?;
        Self {
            address,
            attempts: document.attempts,
            header_bytes: document.header_bytes,
            body_bytes: document.body_bytes,
            request_timeout: Duration::from_millis(document.request_ms),
            service_timeout: Duration::from_millis(document.service_ms),
        }
        .validate()
    }

    pub(crate) fn validate(self) -> Result<Self, Error> {
        if self.address.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST)
            || !(1..=64).contains(&self.attempts)
            || !(64..=8192).contains(&self.header_bytes)
            || !(1..=65_536).contains(&self.body_bytes)
            || self.request_timeout.is_zero()
            || self.request_timeout > Duration::from_secs(5)
            || self.service_timeout.is_zero()
            || self.service_timeout > Duration::from_secs(30)
            || self.request_timeout > self.service_timeout
        {
            return Err(Error::Config);
        }
        Ok(self)
    }

    pub(crate) fn envelope(self) -> Envelope {
        Envelope {
            requests: Limits {
                requests: 1,
                request_bytes: self.body_bytes,
                response_bytes: 1,
                // Registry and worker each retain one bounded decoded input. Transport storage
                // is independently reserved before framing, not charged as guest linear memory.
                buffer_bytes: self.body_bytes + 260 + 1,
                timeout: self.request_timeout,
            },
            memory_bytes: 65_536,
            fuel: 100_000,
            resources: 5,
            callbacks: 8,
        }
    }

    pub(crate) fn buffer_reservation(self) -> usize {
        // At most32 borrowed header-name slots (512 bytes),260 bytes decoded routing metadata,
        // and a fixed <100-byte response. No unbounded map or forwarded headers are retained.
        self.header_bytes + self.body_bytes + 1024
    }
}
