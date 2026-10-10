//! Source, arrangement, grants and envelope are bound before any guest store exists.

use std::sync::{Arc, atomic::Ordering};

use sha2::{Digest, Sha256};
use wasmtime::{Engine, component::Component};
use zryna_backend_webassembly::{ServerOperation, ValidatedServerComponent, pinned_wit_sources};
use zryna_frontend::VerifiedFrontendProvider;
use zryna_source::SourceMap;

use super::{Approval, Envelope, Error, Observation, envelope, grants::Grants};

pub(crate) struct Prepared {
    artifact: ValidatedServerComponent,
    program: zryna_ir::VerifiedProgram,
    sources: SourceMap,
    seal: [u8; 32],
    pub(super) grants: Grants,
    pub(crate) envelope: Envelope,
    pub(super) engine: Engine,
    pub(super) component: Component,
    pub(super) observation: Arc<Observation>,
    pub(super) monotonic_origin: std::time::Instant,
    #[cfg(test)]
    pub(super) pause: Option<Arc<super::host::Pause>>,
    #[cfg(test)]
    pub(super) probe_fuel: Option<u64>,
    #[cfg(test)]
    pub(super) stage_pause: Option<Arc<super::startup::StagePause>>,
}

#[derive(Clone, Copy)]
pub(crate) struct Preparation<'a> {
    pub(crate) export: &'a str,
    pub(crate) operation: ServerOperation,
    pub(crate) document: &'a [u8],
    pub(crate) approval: Approval,
    pub(crate) envelope: Envelope,
}

impl Prepared {
    #[cfg(test)]
    pub(crate) fn new<Provider: VerifiedFrontendProvider>(
        provider: &Provider,
        sources: SourceMap,
        preparation: Preparation<'_>,
        observation: Arc<Observation>,
    ) -> Result<Self, Error> {
        Self::prepare(provider, sources, preparation, observation, false)
    }

    pub(crate) fn new_public<Provider: VerifiedFrontendProvider>(
        provider: &Provider,
        sources: SourceMap,
        preparation: Preparation<'_>,
        observation: Arc<Observation>,
    ) -> Result<Self, Error> {
        Self::prepare(provider, sources, preparation, observation, true)
    }

    fn prepare<Provider: VerifiedFrontendProvider>(
        provider: &Provider,
        sources: SourceMap,
        preparation: Preparation<'_>,
        observation: Arc<Observation>,
        public: bool,
    ) -> Result<Self, Error> {
        // Reject untrusted grants and limits before frontend, WIT, engine or host construction.
        let grants = Grants::admit(preparation.document, preparation.approval)?;
        let envelope = preparation.envelope.validate()?;
        let compiled =
            crate::compile_to_verified_ir(provider, &sources).map_err(|_| Error::Artifact)?;
        let program = compiled.into_program();
        if public && program.functions().count() != 1 {
            return Err(Error::Artifact);
        }
        let artifact = zryna_backend_webassembly::emit_server_response(
            &program,
            &pinned_wit_sources(),
            preparation.export,
            preparation.operation,
        )
        .map_err(|_| Error::Artifact)?;
        artifact.revalidate(&program).map_err(|_| Error::Artifact)?;
        let seal = identity(&artifact, &sources, grants, envelope);
        let engine = envelope::engine()?;
        observation.engines_constructed.fetch_add(1, Ordering::SeqCst);
        let component = Component::new(&engine, artifact.bytes()).map_err(|_| Error::Artifact)?;
        Ok(Self {
            artifact,
            program,
            sources,
            seal,
            grants,
            envelope,
            engine,
            component,
            observation,
            monotonic_origin: std::time::Instant::now(),
            #[cfg(test)]
            pause: None,
            #[cfg(test)]
            probe_fuel: None,
            #[cfg(test)]
            stage_pause: None,
        })
    }

    pub(crate) fn artifact(&self) -> &ValidatedServerComponent {
        &self.artifact
    }

    pub(crate) fn binding(&self) -> &[u8; 32] {
        &self.seal
    }

    pub(super) fn revalidate(&self) -> Result<(), Error> {
        self.artifact.revalidate(&self.program).map_err(|_| Error::Artifact)?;
        if self.seal != identity(&self.artifact, &self.sources, self.grants, self.envelope) {
            return Err(Error::Artifact);
        }
        Ok(())
    }

    /// Deliberately unsealed raw arrangement used only to test runtime interruption/growth.
    /// Normal preparation never accepts replacement component bytes or excessive fuel.
    #[cfg(test)]
    pub(crate) fn runtime_probe(&mut self, grow: bool) -> Result<(), Error> {
        let bytes = super::tests::probes::replace_handle(self.artifact.bytes(), grow);
        self.component = Component::new(&self.engine, bytes).map_err(|_| Error::Artifact)?;
        if !grow {
            self.probe_fuel = Some(1_000_000_000);
        }
        Ok(())
    }
}

fn identity(
    artifact: &ValidatedServerComponent,
    sources: &SourceMap,
    grants: Grants,
    envelope: Envelope,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"zryna.private-server-preparation.v1");
    hash.update(artifact.digest());
    hash.update(artifact.bridge_revision().as_bytes());
    // SourceMap construction already bounds and normalizes every file; length prefixes avoid
    // ambiguity between paths/texts. The verifier-retained program is checked independently.
    for index in 0..sources.len() {
        let index = u32::try_from(index).expect("bounded source map index");
        let id = sources.verify_file_id(index).expect("bounded source map index");
        let file = sources.source(id).expect("source map identity");
        for bytes in [file.path().as_str().as_bytes(), file.text().as_bytes()] {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
    }
    hash.update(grants.identity());
    hash.update(envelope.identity());
    hash.finalize().into()
}
