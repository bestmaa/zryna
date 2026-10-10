//! Exact one-read guest request and separately retained root approval.

use super::{CommandFailure, approval::digest, failure};
use crate::command_request::CapturedFile;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

const WORLD: &str = "zryna:capability-profiles/server@0.1.0";

pub(super) struct Guest {
    request: CapturedFile,
    approval: CapturedFile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    world: String,
    requests: Vec<String>,
    clock: Clock,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Clock {
    monotonic_reads: u32,
    subscriptions: u32,
    timers: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    schema: String,
    request_sha256: String,
    monotonic_reads: u32,
}

impl Guest {
    pub(super) fn capture(request: &Path, approval: &Path) -> Result<Self, CommandFailure> {
        let request = CapturedFile::capture(request, 2048)
            .map_err(|_| failure("ZRYNA-C4201", "Clock guest request is not caller-private."))?;
        let admitted: Request = serde_json::from_slice(request.bytes()).map_err(|_| {
            failure("ZRYNA-C4201", "Clock guest request has invalid or unknown fields.")
        })?;
        if admitted.world != WORLD
            || admitted.requests != ["clock"]
            || admitted.clock.monotonic_reads != 1
            || admitted.clock.subscriptions != 0
            || admitted.clock.timers != 0
        {
            return Err(failure(
                "ZRYNA-C4201",
                "The clock profile requires exactly one monotonic read per request.",
            ));
        }
        let approval = CapturedFile::capture(approval, 1024)
            .map_err(|_| failure("ZRYNA-C4201", "Separate private clock approval is required."))?;
        let granted: Approval = serde_json::from_slice(approval.bytes())
            .map_err(|_| failure("ZRYNA-C4201", "Clock approval has invalid or unknown fields."))?;
        if granted.schema != "zryna.wasi-server-guest-approval.v1"
            || granted.request_sha256 != digest(request.bytes())
            || granted.monotonic_reads != 1
        {
            return Err(failure(
                "ZRYNA-C4201",
                "Clock approval must bind the exact request and approve one read.",
            ));
        }
        Ok(Self { request, approval })
    }

    pub(super) fn document(&self) -> &[u8] {
        self.request.bytes()
    }

    pub(super) fn revalidate(&self) -> Result<(), CommandFailure> {
        self.request.revalidate().and_then(|()| self.approval.revalidate()).map_err(|_| {
            failure("ZRYNA-C4202", "Retained guest request or clock approval changed.")
        })
    }

    pub(super) fn add_record(&self, record: &mut Value) -> Result<(), CommandFailure> {
        let request_identity = self.request.identity().map_err(|_| {
            failure("ZRYNA-C4202", "Retained clock request identity is unavailable.")
        })?;
        let approval_identity = self.approval.identity().map_err(|_| {
            failure("ZRYNA-C4202", "Retained clock approval identity is unavailable.")
        })?;
        record["schema"] = json!("zryna.wasi-server-manifest.v2");
        record["profile"] = json!("server-clock-status-v1");
        record["operation"] = json!("clock_read_then_status");
        record["requested_guest_grants"] = json!(["clock"]);
        record["effective_guest_grants"] = json!(["clock"]);
        record["clock_limits"] = json!({"monotonic_reads":1,"subscriptions":0,"timers":0});
        record["guest_request_sha256"] = json!(digest(self.request.bytes()));
        record["guest_request_identity"] = json!(digest(&request_identity));
        record["guest_approval_identity"] = json!(digest(&approval_identity));
        Ok(())
    }
}
