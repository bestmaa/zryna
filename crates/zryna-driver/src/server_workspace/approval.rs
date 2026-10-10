//! Caller-private configuration and identity-bound root listener permission.

use super::{CommandFailure, failure};
use crate::{command_request::CapturedFile, server_transport::Config};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) struct Inputs {
    configuration: CapturedFile,
    approval: CapturedFile,
    pub(super) config: Config,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    schema: String,
    configuration_sha256: String,
    allow_listen: bool,
}

impl Inputs {
    pub(super) fn capture(config: &Path, approval: &Path) -> Result<Self, CommandFailure> {
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
        Ok(Self { configuration, approval, config })
    }

    pub(super) fn revalidate(&self) -> Result<(), CommandFailure> {
        self.configuration.revalidate().and_then(|()| self.approval.revalidate()).map_err(|_| {
            failure("ZRYNA-C4202", "Retained server configuration or listener approval changed.")
        })
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
