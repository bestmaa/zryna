//! One immutable factory result retaining the complete source/program/core/world binding.

use super::interface_audit::InterfaceTrapSite;
use super::{
    component_audit, interface_audit, language_audit, run_audit::TrapSite, shell, storage,
};
use crate::{WitSource, WitWorldAudit, wit_world_audit::AuthenticatedCommandWorld};
use sha2::{Digest, Sha256};
use zryna_diagnostics::Diagnostic;
use zryna_ir::command_h1_v1::{ProgramIdentity, VerifiedProgram};
use zryna_ownership_runtime_abi::VerifiedOwnershipRuntimeAbi;
use zryna_source::SourceMap;
mod fingerprint;
#[cfg(test)]
mod tests;

/// An immutable command artifact produced only from complete verified command authorities.
///
/// Its content observations confer no host grants. Ordinary ownership programs cannot enter:
///
/// ```compile_fail
/// fn substitute(program: &zryna_ir::data_ownership_v1::VerifiedProgram) {
///     let _: &zryna_ir::command_h1_v1::VerifiedProgram = program;
/// }
/// ```
///
/// ```compile_fail
/// let artifact = zryna_backend_webassembly::ValidatedCommandH1Artifact { bytes: Vec::new() };
/// ```
pub struct Artifact {
    bytes: Vec<u8>,
    language: Vec<u8>,
    storage: Vec<u8>,
    world: AuthenticatedCommandWorld,
    program: VerifiedProgram,
    source_digest: [u8; 32],
    language_digest: [u8; 32],
    storage_digest: [u8; 32],
    component_digest: [u8; 32],
    world_digest: [u8; 32],
    wit_closure_digest: [u8; 32],
    program_binding: [u8; 32],
    traps: Vec<TrapSite>,
    interface_traps: Vec<InterfaceTrapSite>,
}

impl Artifact {
    /// Emits and independently audits storage, language, component graph and pinned WIT.
    ///
    /// # Errors
    /// Rejects mismatched source, program, ownership ABI or WIT authority, or a final-byte audit.
    pub fn emit(
        program: &VerifiedProgram,
        runtime: &VerifiedOwnershipRuntimeAbi,
        sources: &SourceMap,
        wit: &[WitSource],
    ) -> Result<Self, Diagnostic> {
        if sources.len() != 1
            || !program.source().is_bound_to(sources)
            || program.type_universe_identity() != runtime.type_universe_identity()
            || program.linear32_layouts().fingerprint() != &runtime.linear32_fingerprint()
            || program.linux_x86_64_layouts().fingerprint() != &runtime.linux_x86_64_fingerprint()
            || runtime.webassembly_functions().len() != runtime.operations().len()
        {
            return Err(invalid());
        }
        let source = program
            .source()
            .syntax()
            .files()
            .first()
            .and_then(|file| sources.source(file.id()))
            .ok_or_else(invalid)?;
        let source_digest = Sha256::digest(source.text().as_bytes()).into();
        let world = AuthenticatedCommandWorld::new(wit)?;
        let storage = storage::encode();
        storage::audit::audit(&storage)?;
        let language = crate::data_ownership_v1::encode::command(program)?;
        let mut traps = language_audit::audit(&language, program)?;
        let bytes = shell::encode(&world, &storage, &language)?;
        let language_base = component_audit::audit(&bytes, &storage, &language, &world)?;
        let interface_traps = interface_audit::audit(&storage, &bytes)?;
        for trap in &mut traps {
            trap.module_offset =
                trap.module_offset.checked_add(language_base).ok_or_else(invalid)?;
        }
        let language_digest = Sha256::digest(&language).into();
        let storage_digest = Sha256::digest(&storage).into();
        let component_digest = Sha256::digest(&bytes).into();
        let world_digest = fingerprint::world(wit, world.audit())?;
        let wit_closure_digest = fingerprint::closure(wit)?;
        let mut key = None;
        if let Some(requirement) = program.source().environment() {
            key = Some(requirement.key());
        }
        let program_binding = fingerprint::program(&fingerprint::Inputs {
            source: source_digest,
            language: language_digest,
            storage: storage_digest,
            linear: *program.linear32_layouts().fingerprint(),
            linux: *program.linux_x86_64_layouts().fingerprint(),
            world: world_digest,
            key,
        })?;
        Ok(Self {
            bytes,
            language,
            storage,
            world,
            program: program.clone(),
            source_digest,
            language_digest,
            storage_digest,
            component_digest,
            world_digest,
            wit_closure_digest,
            program_binding,
            traps,
            interface_traps,
        })
    }

    /// Returns complete independently audited component bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Returns the sealed language core embedded in the component.
    #[must_use]
    pub fn language(&self) -> &[u8] {
        &self.language
    }
    /// Returns the independently audited shared storage core.
    #[must_use]
    pub fn storage(&self) -> &[u8] {
        &self.storage
    }
    /// Returns retained authenticated world observations.
    #[must_use]
    pub fn world(&self) -> &WitWorldAudit {
        self.world.audit()
    }
    /// Returns the immutable command program retained by the factory.
    #[must_use]
    pub fn program(&self) -> &VerifiedProgram {
        &self.program
    }
    /// Returns the exact retained program issuing identity.
    #[must_use]
    pub fn issuer(&self) -> ProgramIdentity {
        self.program.identity()
    }
    /// Returns the source UTF-8 SHA-256 observation.
    #[must_use]
    pub fn source_digest(&self) -> &[u8; 32] {
        &self.source_digest
    }
    /// Returns the embedded language core SHA-256 observation.
    #[must_use]
    pub fn language_digest(&self) -> &[u8; 32] {
        &self.language_digest
    }
    /// Returns the embedded storage core SHA-256 observation.
    #[must_use]
    pub fn storage_digest(&self) -> &[u8; 32] {
        &self.storage_digest
    }
    /// Returns the complete component SHA-256 observation.
    #[must_use]
    pub fn component_digest(&self) -> &[u8; 32] {
        &self.component_digest
    }
    /// Returns the versioned authenticated WIT content binding.
    #[must_use]
    pub fn world_digest(&self) -> &[u8; 32] {
        &self.world_digest
    }
    /// Returns the versioned source/body/target binding, which is not execution authority.
    #[must_use]
    pub fn program_binding(&self) -> &[u8; 32] {
        &self.program_binding
    }
    /// Returns the separately framed exact pinned WIT closure SHA-256 observation.
    #[must_use]
    pub fn wit_closure_digest(&self) -> &[u8; 32] {
        &self.wit_closure_digest
    }
    /// Returns audited controlled-language sites in component-relative byte coordinates.
    /// A runtime must also authenticate the originating compiled component and language core.
    #[must_use]
    pub fn traps(&self) -> &[TrapSite] {
        &self.traps
    }

    /// Returns audited canonical-transfer failures in component-relative byte coordinates.
    /// A runtime must also authenticate the originating compiled component and storage core.
    #[must_use]
    pub fn interface_traps(&self) -> &[InterfaceTrapSite] {
        &self.interface_traps
    }

    /// Revalidates retained bytes against the exact issuing program and paired witnesses.
    ///
    /// # Errors
    /// Rejects a fresh issuer, stale source, foreign ABI, changed WIT or substituted bytes.
    pub fn revalidate(
        &self,
        program: &VerifiedProgram,
        runtime: &VerifiedOwnershipRuntimeAbi,
        sources: &SourceMap,
        wit: &[WitSource],
    ) -> Result<(), Diagnostic> {
        if self.issuer() != program.identity() {
            return Err(invalid());
        }
        let candidate = Self::emit(program, runtime, sources, wit)?;
        if self.bytes != candidate.bytes
            || self.language != candidate.language
            || self.storage != candidate.storage
            || self.program_binding != candidate.program_binding
            || self.source_digest != candidate.source_digest
            || self.language_digest != candidate.language_digest
            || self.storage_digest != candidate.storage_digest
            || self.component_digest != candidate.component_digest
            || self.world_digest != candidate.world_digest
            || self.wit_closure_digest != candidate.wit_closure_digest
            || self.world.audit() != candidate.world.audit()
            || self.traps != candidate.traps
            || self.interface_traps != candidate.interface_traps
        {
            return Err(invalid());
        }
        Ok(())
    }
}

fn invalid() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4100",
        None,
        "Command source, program, layout or runtime authority does not match.",
        "Emit the complete command with its retained source map and paired verified ownership ABI.",
    )
}
