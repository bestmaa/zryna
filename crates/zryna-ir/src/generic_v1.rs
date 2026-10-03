//! Independent closed-key and typed-graph checks for the successor generic IR.
//!
//! These checks return no backend-consumable program. Exact original syntax and substitution
//! are checked by the source-bound entrypoint. Complete source operation coverage,
//! ownership/runtime authority and the new wire protocol remain mandatory prerequisites to
//! executable sealing. The separate `copy_v1` lane proves these obligations only for immutable
//! Copy values after the new wire gate. Existing M1/M2/M3 constructors are unchanged.

use zryna_diagnostics::Diagnostic;

mod body;
mod calls;
mod cfg;
pub mod copy_v1;
mod inventory;
pub mod keys;
mod loops;
pub mod raw;
mod source;
mod source_body;
mod source_calls;
mod source_types;
mod substitution;
#[cfg(test)]
mod tests;
pub mod wire;

/// Atomic successor validation failure.
#[derive(Debug)]
pub enum Failure {
    /// A complete claim was rejected without any executable authority.
    Diagnostics(Vec<Diagnostic>),
    /// Fallible temporary storage could not be reserved.
    AllocationFailure,
    /// Checked internal arithmetic or graph invariant failed.
    InternalFailure,
}

fn reject(message: &str) -> Failure {
    Failure::Diagnostics(vec![Diagnostic::error(
        "ZRYNA-I7001",
        None,
        message,
        "provide exact closed successor claims from authenticated source",
    )])
}

fn budget(message: &str) -> Failure {
    Failure::Diagnostics(vec![Diagnostic::error(
        "ZRYNA-I3201",
        None,
        message,
        "stay within the inherited and closed-instantiation ceilings",
    )])
}

fn reserve<T>(count: usize) -> Result<Vec<T>, Failure> {
    let mut result = Vec::new();
    result.try_reserve_exact(count).map_err(|_| Failure::AllocationFailure)?;
    Ok(result)
}

/// Checks complete closed inventory, signatures, typed operations, CFG and active-payload edges.
///
/// This returns no identity or executable authority. It does not establish authenticated
/// original-declaration substitution, ownership/loan/drop plans, wire admission or runtime proof.
///
/// The completed check is deliberately not a program seal.
/// ```compile_fail
/// fn execute(program: &zryna_ir::generic_v1::raw::Program,
///     sources: &zryna_source::SourceMap,
///     linear: &zryna_layout::generic_v1::VerifiedLayouts,
///     linux: &zryna_layout::generic_v1::VerifiedLayouts) {
///     let check = zryna_ir::generic_v1::validate_closed_graph(program, sources, linear, linux).unwrap();
///     let _: zryna_ir::data_ownership_v1::VerifiedProgram = check;
/// }
/// ```
///
/// # Errors
/// Rejects foreign layout/source authority, malformed keys, signatures, calls, variants, values,
/// graph dominance and resource amplification without any partially sealed program.
pub fn validate_closed_graph(
    program: &raw::Program,
    sources: &zryna_source::SourceMap,
    linear: &zryna_layout::generic_v1::VerifiedLayouts,
    linux: &zryna_layout::generic_v1::VerifiedLayouts,
) -> Result<(), Failure> {
    let inventory = inventory::check(program, sources, linear, linux)?;
    body::check(program, sources, linear, &inventory)
}

/// Independently checks original declarations, closed signatures, claimed call targets and nominal payloads against
/// exact authenticated v5 syntax, in addition to the complete typed-graph checks.
///
/// This is still a validation result, not an executable program seal. Source operation coverage,
/// ownership/loan/drop plans, successor wire admission and runtime proof remain required.
///
/// # Errors
/// Rejects foreign syntax before traversing raw claims, fabricated original metadata, source
/// name/import mismatches, incorrect signature/call substitution and source-inconsistent sealed layouts.
pub fn validate_source_graph(
    program: &raw::Program,
    syntax: &zryna_syntax::v5::VerifiedProjectSyntaxV5,
    sources: &zryna_source::SourceMap,
    linear: &zryna_layout::generic_v1::VerifiedLayouts,
    linux: &zryna_layout::generic_v1::VerifiedLayouts,
) -> Result<(), Failure> {
    if !syntax.is_bound_to(sources) {
        return Err(reject("syntax authority belongs to a different immutable source map"));
    }
    let inventory = inventory::check(program, sources, linear, linux)?;
    let originals = source::Originals::check(program, syntax)?;
    substitution::check(program, linear, &originals)?;
    source_calls::check(program, &originals)?;
    body::check(program, sources, linear, &inventory)
}
