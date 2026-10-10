//! Private process-corpus protocol. This is not registered in the shipped CLI.
//! Input source bytes are explicit stdin data; there is no duplicate filesystem admission path.

use super::{
    Bound, Config, Error,
    io::Observation,
    source::{CapturedSource, prepare_source},
};
use crate::server_runtime::Observation as RuntimeObservation;
use serde::Deserialize;
use std::sync::Arc;
use zryna_frontend::WorkerFrontend;

pub(super) const MAX_INPUT: u64 = 70 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    selector: String,
    configuration: String,
    source: String,
    export: String,
}

pub(super) struct Command {
    config: Config,
    captured: CapturedSource,
    export: String,
}
impl Command {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() as u64 > MAX_INPUT {
            return Err(Error::Limit);
        }
        let document: Document = serde_json::from_slice(bytes).map_err(|_| Error::Config)?;
        if document.selector != "server-loopback-private-v1"
            || document.source.len() > 65_536
            || document.export.is_empty()
            || document.export.len() > 128
            || !document.export.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(Error::Config);
        }
        let config = Config::parse(document.configuration.as_bytes())?;
        let captured = CapturedSource::from_bytes("status.zry", document.source.into_bytes())?;
        Ok(Self { config, captured, export: document.export })
    }
    pub(super) fn start(
        self,
        frontend: &WorkerFrontend,
        transport: Arc<Observation>,
        runtime: Arc<RuntimeObservation>,
    ) -> Result<Bound, Error> {
        let prepared =
            prepare_source(frontend, self.captured, &self.export, self.config.envelope(), runtime)?;
        Bound::start(self.config, prepared, transport)
    }
}
