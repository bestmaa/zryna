//! Command semantics retain a distinct mandatory IR authority and no ordinary M3 accessor.

use zryna_diagnostics::Diagnostic;
use zryna_ir::command_h1_v1 as ir;
use zryna_ownership_runtime_abi::VerifiedOwnershipRuntimeAbi;
use zryna_source::SourceMap;
use zryna_syntax::command_h1_v1::CommandSyntax;

/// Command compiler result with exact owned-body and runtime declaration authorities.
///
/// ```compile_fail
/// fn substitute(command: &zryna_semantics::command_h1_v1::VerifiedProgram) {
///     let _: &zryna_ir::data_ownership_v1::VerifiedProgram = command.verified_ir();
/// }
/// ```
#[derive(Clone, Debug)]
pub struct VerifiedProgram {
    ir: ir::VerifiedProgram,
    runtime_abi: VerifiedOwnershipRuntimeAbi,
}

impl VerifiedProgram {
    /// Returns the distinct command IR authority.
    #[must_use]
    pub const fn verified_ir(&self) -> &ir::VerifiedProgram {
        &self.ir
    }

    /// Returns the shared ownership ABI declaration, without granting a host capability.
    #[must_use]
    pub const fn runtime_abi(&self) -> &VerifiedOwnershipRuntimeAbi {
        &self.runtime_abi
    }
}

/// Lowers only complete authenticated command source and immediately seals command IR.
///
/// # Errors
/// Rejects stale source, unsupported semantic forms, types, ownership, entry, effects or cleanup.
pub fn lower(
    source: &CommandSyntax,
    sources: &SourceMap,
) -> Result<VerifiedProgram, Vec<Diagnostic>> {
    let (ir, runtime_abi) = crate::data_ownership_v1::command_support::lower(source, sources)?;
    Ok(VerifiedProgram { ir, runtime_abi })
}

#[cfg(test)]
mod tests;
