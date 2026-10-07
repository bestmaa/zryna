//! Runtime-bound core Wasm 1.0 for the separately sealed immutable generic owned lane.
//!
//! Aggregates use private i32 lanes. Private return globals avoid multi-value features and
//! allocation; callers immediately capture returned lanes. Scalar wrappers prevent host reentry.
//! Exact trusted runtime imports share bounded memory; private calls propagate cleanup statuses.

use crate::ValidatedWebAssemblyArtifact;
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::owned_v2::VerifiedOwnedProgram;

mod audit;
mod bytes;
mod control;
mod effects;
mod encode;
mod layout;
mod runtime;
#[cfg(test)]
mod tests;

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_TYPE_LANES: u32 = 65_536;
const MAX_FUNCTION_LOCALS: u32 = 1_048_576;

/// Emits and independently audits only an authenticated immutable generic owned program.
///
/// ```compile_fail
/// fn raw(program: &zryna_ir::generic_v1::raw::Program) {
///     zryna_backend_webassembly::generic_owned_v2::emit(program);
/// }
/// ```
///
/// # Errors
/// Rejects lane/output amplification, invalid final bytes, or sealed-program invariant drift.
pub fn emit(
    program: &VerifiedOwnedProgram<'_>,
) -> Result<ValidatedWebAssemblyArtifact, Diagnostic> {
    let layout = layout::Layout::new(program)?;
    let bytes = encode::module(&layout)?;
    audit::seal(bytes, &layout)
}

fn budget() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4001",
        None,
        "generic owned Wasm exceeds its lane, local, or 32 MiB artifact budget",
        "reduce the sealed owned program below the documented Wasm amplification limits",
    )
}

fn invariant() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4002",
        None,
        "generic owned Wasm emission invariant failed",
        "retain the exact sealed successor program and its Linear32 layout authority",
    )
}
