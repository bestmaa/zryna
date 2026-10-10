//! Caller-private configuration and identity-bound root listener permission.

use super::{CommandFailure, failure};
use crate::{command_request::CapturedFile, server_runtime, server_transport::Config};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) struct Inputs {
    configuration: CapturedFile,
    approval: CapturedFile,
    pub(super) config: Config,
    pub(super) guest: Option<super::guest::Guest>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    schema: String,
    configuration_sha256: String,
    allow_listen: bool,
}

impl Inputs {
    pub(super) fn capture(
        config: &Path,
        approval: &Path,
        guest: Option<(&Path, &Path)>,
    ) -> Result<Self, CommandFailure> {
        let configuration = CapturedFile::capture(config, 1024)
            .map_err(|_| failure("ZRYNA-C4201", "Server configuration is not caller-private."))?;
        let config = Config::parse(configuration.bytes()).map_err(|_| {
            failure("ZRYNA-C4201", "Server configuration exceeds the closed loopback limits.")
        })?;
        let approval = CapturedFile::capture(approval, 1024).map_err(|_| {
            failure("ZRYNA-C4201", "Explicit private listener approval is required.")
        })?;
        let admitted: Approval = serde_json::from_slice(approval.bytes()).map_err(|_| {
            failure("ZRYNA-C4201", "Listener approval has invalid or unknown fields.")
        })?;
        if admitted.schema != "zryna.wasi-server-listener-approval.v1"
            || !admitted.allow_listen
            || admitted.configuration_sha256 != digest(configuration.bytes())
        {
            return Err(failure(
                "ZRYNA-C4201",
                "Listener approval must bind the exact configuration bytes.",
            ));
        }
        let guest = guest
            .map(|(request, approval)| super::guest::Guest::capture(request, approval))
            .transpose()?;
        Ok(Self { configuration, approval, config, guest })
    }

    pub(super) fn revalidate(&self) -> Result<(), CommandFailure> {
        self.configuration.revalidate().and_then(|()| self.approval.revalidate()).map_err(
            |_| {
                failure(
                    "ZRYNA-C4202",
                    "Retained server configuration or listener approval changed.",
                )
            },
        )?;
        self.guest.as_ref().map_or(Ok(()), super::guest::Guest::revalidate)
    }

    pub(super) fn document(&self) -> &[u8] {
        self.guest.as_ref().map_or(
            br#"{"world":"zryna:capability-profiles/server@0.1.0","requests":[]}"#,
            super::guest::Guest::document,
        )
    }

    pub(super) fn operation(&self) -> zryna_backend_webassembly::ServerOperation {
        if self.guest.is_some() {
            zryna_backend_webassembly::ServerOperation::ClockRead
        } else {
            zryna_backend_webassembly::ServerOperation::Reply
        }
    }

    pub(super) fn host_approval(&self) -> server_runtime::Approval {
        if self.guest.is_some() {
            server_runtime::Approval::one_monotonic_clock_read()
        } else {
            server_runtime::Approval::deny_all()
        }
    }

    pub(super) fn manifest_name(&self) -> &'static str {
        if self.guest.is_some() { super::record::CLOCK_NAME } else { super::record::NAME }
    }

    pub(super) fn configuration_digest(&self) -> String {
        digest(self.configuration.bytes())
    }
    pub(super) fn approval_identity(&self) -> Result<String, CommandFailure> {
        self.approval
            .identity()
            .map(|bytes| digest(&bytes))
            .map_err(|_| failure("ZRYNA-C4202", "Listener approval identity is unavailable."))
    }
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
