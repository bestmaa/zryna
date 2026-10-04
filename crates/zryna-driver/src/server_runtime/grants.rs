//! Exact server-only request/approval admission; not the command H1 grant interface.

use serde::Deserialize;

use super::Error;

pub(super) const WORLD: &str = "zryna:capability-profiles/server@0.1.0";
const MAX_REQUEST_BYTES: usize = 2048;
const MAX_CLOCK_READS: u32 = 16;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    world: String,
    requests: Vec<String>,
    clock: Option<Clock>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Clock {
    monotonic_reads: u32,
    subscriptions: u32,
    timers: u32,
}

/// Immutable internal admission token. Only explicitly approved read-only clock access exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Grants {
    clock_reads: u32,
}

/// Host-selected approval is separate from the untrusted request document.
#[derive(Clone, Copy)]
pub(super) struct Approval {
    clock_reads: u32,
}

impl Approval {
    pub(super) const fn deny_all() -> Self {
        Self { clock_reads: 0 }
    }

    pub(super) fn monotonic_clock_reads(reads: u32) -> Result<Self, Error> {
        if reads == 0 || reads > MAX_CLOCK_READS {
            return Err(Error::Grant);
        }
        Ok(Self { clock_reads: reads })
    }
}

impl Grants {
    pub(super) fn admit(bytes: &[u8], approval: Approval) -> Result<Self, Error> {
        if bytes.is_empty() || bytes.len() > MAX_REQUEST_BYTES {
            return Err(Error::Grant);
        }
        let request: Request = serde_json::from_slice(bytes).map_err(|_| Error::Grant)?;
        if request.world != WORLD
            || request.requests.len() > 1
            || request.requests.iter().any(|capability| capability != "clock")
        {
            return Err(Error::Grant);
        }
        match (request.requests.as_slice(), request.clock) {
            ([], None) => Ok(Self { clock_reads: 0 }),
            ([capability], Some(clock))
                if capability == "clock"
                    && clock.monotonic_reads > 0
                    && clock.monotonic_reads <= MAX_CLOCK_READS
                    && clock.monotonic_reads <= approval.clock_reads
                    && clock.subscriptions == 0
                    && clock.timers == 0 =>
            {
                Ok(Self { clock_reads: clock.monotonic_reads })
            }
            _ => Err(Error::Grant),
        }
    }

    pub(super) const fn clock_reads(self) -> u32 {
        self.clock_reads
    }

    pub(super) fn identity(self) -> [u8; 4] {
        self.clock_reads.to_le_bytes()
    }
}
