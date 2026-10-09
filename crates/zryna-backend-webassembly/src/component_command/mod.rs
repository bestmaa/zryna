//! Private command arrangement over the unchanged authenticated command world.

use sha2::{Digest, Sha256};
use zryna_diagnostics::Diagnostic;
use zryna_ir::VerifiedProgram;

use crate::{
    ValidatedWebAssemblyArtifact, WitSource, WitWorldAudit,
    wit_world_audit::AuthenticatedCommandWorld,
};

mod audit;
mod bridge;
mod bridge_audit;
mod graph_budget;
mod shell;
pub(crate) mod type_budget;
pub(crate) mod type_graph;
mod type_indices;

#[cfg(test)]
mod tests;

const BRIDGE_REVISION: &str = "zryna.command-self-check.v1";

/// A separately sealed command component; its nested scalar core remains a core-only artifact.
///
/// This internal proof slice does not activate a public command target or grant host capabilities.
pub struct ValidatedCommandComponent {
    bytes: Vec<u8>,
    core: ValidatedWebAssemblyArtifact,
    world: AuthenticatedCommandWorld,
    invocation: bridge::CommandInvocation,
    logical_export: String,
    digest: [u8; 32],
    core_digest: [u8; 32],
}

impl ValidatedCommandComponent {
    /// Returns the complete independently audited component bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the exact compiler-produced core retained inside the component.
    #[must_use]
    pub const fn core(&self) -> &ValidatedWebAssemblyArtifact {
        &self.core
    }

    /// Returns the authenticated explicit and resolved WIT world observations.
    #[must_use]
    pub fn world_audit(&self) -> &WitWorldAudit {
        self.world.audit()
    }

    /// Returns the private application bridge revision, independently of the WIT world version.
    #[must_use]
    pub const fn bridge_revision(&self) -> &'static str {
        BRIDGE_REVISION
    }

    /// Returns the digest of the complete retained component.
    #[must_use]
    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    /// Returns the digest of the unchanged scalar core.
    #[must_use]
    pub const fn core_digest(&self) -> &[u8; 32] {
        &self.core_digest
    }

    /// Revalidates the sealed bytes against the driver's matching immutable verified program.
    ///
    /// # Errors
    ///
    /// Rejects a substituted program, scalar ABI, core, invocation, world or component.
    pub fn revalidate(&self, program: &VerifiedProgram) -> Result<(), Diagnostic> {
        let invocation = bridge::CommandInvocation::new(
            program,
            &self.logical_export,
            self.invocation.arguments(),
            self.invocation.expected(),
        )?;
        if invocation != self.invocation
            || crate::emit(program)?.bytes() != self.core.bytes()
            || digest(&self.bytes) != self.digest
            || digest(self.core.bytes()) != self.core_digest
        {
            return Err(Diagnostic::error(
                "ZRYNA-W4016",
                None,
                "command component binding no longer matches its verified scalar program",
                "compile and seal the command from one matching verified source instance",
            ));
        }
        audit::audit(&self.bytes, &self.core, &invocation, &self.world)
    }
}

/// Compiles and independently audits the private scalar self-check command arrangement.
///
/// # Errors
///
/// Rejects unauthenticated WIT sources, unsupported scalar invocations, exceeded envelopes,
/// or any mismatch found by the independent final-byte auditor.
pub fn emit_command_self_check(
    program: &VerifiedProgram,
    sources: &[WitSource],
    export: &str,
    arguments: &[i32],
    expected: i32,
) -> Result<ValidatedCommandComponent, Diagnostic> {
    let world = AuthenticatedCommandWorld::new(sources)?;
    let invocation = bridge::CommandInvocation::new(program, export, arguments, expected)?;
    let core = crate::emit(program)?;
    let bridge = bridge::encode(&invocation);
    let bytes = shell::encode(&world, &core, &bridge)?;
    audit::audit(&bytes, &core, &invocation, &world)?;
    Ok(ValidatedCommandComponent {
        digest: digest(&bytes),
        core_digest: digest(core.bytes()),
        bytes,
        core,
        world,
        invocation,
        logical_export: export.to_owned(),
    })
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
