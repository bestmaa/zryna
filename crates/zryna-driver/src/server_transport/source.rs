//! One owned source snapshot, compiled through the normal authenticated production worker.

use std::sync::Arc;

use zryna_backend_webassembly::ServerOperation;
use zryna_frontend::WorkerFrontend;
use zryna_source::{MAX_SOURCE_FILE_BYTES, SourceFileInput, SourceMap};

use crate::server_runtime::{Approval, Envelope, Error, Observation, Preparation, Prepared};

const EMPTY_REQUESTS: &[u8] =
    br#"{"world":"zryna:capability-profiles/server@0.1.0","requests":[]}"#;

/// Owns the exact bounded bytes captured by transport initialization. No file handle or path
/// authority survives capture, and preparation/serving never reread a filesystem source.
pub(super) struct CapturedSource {
    sources: SourceMap,
}

impl CapturedSource {
    pub(super) fn from_bytes(logical_path: &str, bytes: Vec<u8>) -> Result<Self, Error> {
        if bytes.len() > MAX_SOURCE_FILE_BYTES {
            return Err(Error::Artifact);
        }
        let text = String::from_utf8(bytes).map_err(|_| Error::Artifact)?;
        let sources =
            SourceMap::build(vec![SourceFileInput { path: logical_path.to_owned(), text }])
                .map_err(|_| Error::Artifact)?;
        Ok(Self { sources })
    }
}

/// Only a source-produced empty-body response can enter this transport seam. The authenticated
/// worker, source verifier, semantic lowering, IR verifier and final component audit all run in
/// normal preparation; this entry point accepts no raw syntax/program/component or grant input.
pub(super) fn prepare_source(
    frontend: &WorkerFrontend,
    captured: CapturedSource,
    export: &str,
    envelope: Envelope,
    observation: Arc<Observation>,
) -> Result<Prepared, Error> {
    Prepared::new(
        frontend,
        captured.sources,
        Preparation {
            export,
            operation: ServerOperation::Reply,
            document: EMPTY_REQUESTS,
            approval: Approval::deny_all(),
            envelope,
        },
        observation,
    )
}

#[cfg(test)]
#[path = "source_tests.rs"]
pub(super) mod tests;
