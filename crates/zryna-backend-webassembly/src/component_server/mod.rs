//! Private, authenticated empty-body WASI HTTP arrangement over a verified scalar core.

use sha2::{Digest, Sha256};
use zryna_diagnostics::Diagnostic;
use zryna_ir::{Type, VerifiedProgram};

use crate::{ValidatedWebAssemblyArtifact, WitSource, WitWorldAudit};

mod audit;
mod bridge;
mod memory;
mod shell;
mod world;

#[cfg(test)]
mod tests;

const REVISION: &str = "zryna.server-empty-response.v1";

/// Fixed private host arrangements; neither variant admits a public server profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerOperation {
    /// Serve one empty response whose status is computed by the retained scalar core.
    Reply,
    /// Additionally read the monotonic clock once as an internal host-policy proof.
    /// This does not represent source-level clock requirement admission.
    ClockRead,
}

/// Sealed exact-world component retaining its compiler-produced scalar program and arrangement.
pub struct ValidatedServerComponent {
    bytes: Vec<u8>,
    core: ValidatedWebAssemblyArtifact,
    world: world::World,
    logical_export: String,
    core_export: String,
    operation: ServerOperation,
    digest: [u8; 32],
}

impl ValidatedServerComponent {
    /// Complete, independently validated executable component bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The unchanged compiler-produced scalar core embedded in the component.
    #[must_use]
    pub const fn core(&self) -> &ValidatedWebAssemblyArtifact {
        &self.core
    }

    /// Authenticated observations for the pinned source closure, including the exact server world.
    #[must_use]
    pub fn world_audit(&self) -> &WitWorldAudit {
        self.world.audit()
    }

    /// Complete component digest, separate from the retained program identity.
    #[must_use]
    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    /// Revision of the fixed private response arrangement.
    #[must_use]
    pub const fn bridge_revision(&self) -> &'static str {
        REVISION
    }

    /// The immutable, fixed host operation arrangement.
    #[must_use]
    pub const fn operation(&self) -> ServerOperation {
        self.operation
    }

    /// Revalidate the source binding, complete bytes, core, topology and exact authenticated world.
    ///
    /// # Errors
    /// Rejects any substituted program, export, arrangement or component before host construction.
    pub fn revalidate(&self, program: &VerifiedProgram) -> Result<(), Diagnostic> {
        if scalar_export(program, &self.logical_export)? != self.core_export
            || crate::emit(program)?.bytes() != self.core.bytes()
            || digest(&self.bytes) != self.digest
        {
            return Err(invalid(
                "server component no longer matches its retained verified program",
            ));
        }
        audit::audit(&self.bytes, &self.core, &self.world, &self.core_export, self.operation)
    }
}

/// Emit one fixed empty-body incoming-handler component from a matching verified scalar export.
///
/// The no-argument i32 export computes the response status. The guest traps if it is outside
/// 200–599. Every external host capability remains subject to the driver's separate admission.
///
/// # Errors
/// Rejects substituted WIT, unsupported scalar signatures, excessive bytes or any audit mismatch.
pub fn emit_server_response(
    program: &VerifiedProgram,
    sources: &[WitSource],
    export: &str,
    operation: ServerOperation,
) -> Result<ValidatedServerComponent, Diagnostic> {
    let core_export = scalar_export(program, export)?;
    let world = world::World::new(sources)?;
    let core = crate::emit(program)?;
    let bytes = shell::encode(&world, &core, &core_export, operation)?;
    audit::audit(&bytes, &core, &world, &core_export, operation)?;
    Ok(ValidatedServerComponent {
        digest: digest(&bytes),
        bytes,
        core,
        world,
        logical_export: export.to_owned(),
        core_export,
        operation,
    })
}

fn scalar_export(program: &VerifiedProgram, export: &str) -> Result<String, Diagnostic> {
    let function =
        program
            .functions()
            .find(|function| function.export_name().as_str() == export)
            .ok_or_else(|| invalid("server status export is absent from the verified program"))?;
    let mapped = function.abi_export().webassembly_name().as_str();
    if !function.parameters().is_empty()
        || function.return_type() != Type::I32
        || export.len() > 256
        || mapped.len() > 256
    {
        return Err(invalid("server status export must have the exact no-argument i32 signature"));
    }
    Ok(mapped.to_owned())
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn invalid(message: &str) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4030",
        None,
        message,
        "restore the pinned server world and matching bounded private response arrangement",
    )
}
