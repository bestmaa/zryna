//! Import-free core Wasm 1.0 for the separately sealed immutable generic Copy lane.
//!
//! Aggregates use private i32 lanes. Private return globals avoid multi-value features and
//! allocation; callers immediately copy all returned lanes into their own locals. No imports
//! allow host reentry, and every successful return initializes every lane of its result.

use crate::ValidatedWebAssemblyArtifact;
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::copy_v1::VerifiedCopyProgram;

mod audit;
mod bytes;
mod control;
mod encode;
mod layout;
#[cfg(test)]
mod tests;

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_TYPE_LANES: u32 = 65_536;
const MAX_FUNCTION_LOCALS: u32 = 1_048_576;

/// Emits and independently audits only an authenticated immutable generic Copy program.
///
/// ```compile_fail
/// fn raw(program: &zryna_ir::generic_v1::raw::Program) {
///     zryna_backend_webassembly::generic_copy_v1::emit(program);
/// }
/// ```
///
/// # Errors
/// Rejects lane/output amplification, invalid final bytes, or sealed-program invariant drift.
pub fn emit(program: &VerifiedCopyProgram<'_>) -> Result<ValidatedWebAssemblyArtifact, Diagnostic> {
    let layout = layout::Layout::new(program)?;
    let bytes = encode::module(&layout)?;
    audit::seal(bytes, &layout)
}

fn budget() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4001",
        None,
        "generic Copy Wasm exceeds its lane, local, or 32 MiB artifact budget",
        "reduce the sealed Copy program below the documented Wasm amplification limits",
    )
}

fn invariant() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4002",
        None,
        "generic Copy Wasm emission invariant failed",
        "retain the exact sealed successor program and its Linear32 layout authority",
    )
}
