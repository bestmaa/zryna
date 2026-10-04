//! Separate source-bound owned/loan/drop successor; frozen Copy bytes remain unchanged.
use super::{Failure, raw as graph, reject, source::Originals};
use zryna_layout::{StorageTarget, generic_v1::VerifiedLayouts};
use zryna_ownership_runtime_abi::generic_v1::VerifiedOwnershipRuntimeAbi;
use zryna_source::{FileId, SourceMap};
use zryna_syntax::v5::VerifiedProjectSyntaxV5;
mod originals;
mod plan;
mod producer;
pub mod raw;
mod source;
mod typed;
pub mod wire;
pub use originals::check_original_bodies;
pub use producer::produce_claim;
/// Immutable exact source/layout/runtime/ABI-bound owned program.
/// ```compile_fail
/// fn forge(raw: zryna_ir::generic_v1::owned_v2::raw::Program) {
///     let _: zryna_ir::generic_v1::owned_v2::VerifiedOwnedProgram<'_> = raw;
/// }
/// ```
#[derive(Debug)]
pub struct VerifiedOwnedProgram<'a> {
    claim: raw::Program,
    linear: &'a VerifiedLayouts,
    linux: &'a VerifiedLayouts,
    runtime: &'a VerifiedOwnershipRuntimeAbi<'a>,
    abi: zryna_abi::VerifiedScalarAbiModule,
    exports: Vec<usize>,
}
impl<'a> VerifiedOwnedProgram<'a> {
    /// Complete authenticated functions in unsigned key order.
    #[must_use]
    pub fn functions(&self) -> &[graph::Function] {
        &self.claim.graph.functions
    }
    /// Exact retained successor layouts.
    #[must_use]
    pub const fn layouts(&self, target: StorageTarget) -> &'a VerifiedLayouts {
        match target {
            StorageTarget::Linear32V1 => self.linear,
            StorageTarget::LinuxX8664V1 => self.linux,
        }
    }
    /// Exact retained runtime declarations.
    #[must_use]
    pub const fn runtime(&self) -> &'a VerifiedOwnershipRuntimeAbi<'a> {
        self.runtime
    }
    /// Unchanged scalar export ABI.
    #[must_use]
    pub const fn scalar_abi(&self) -> &zryna_abi::VerifiedScalarAbiModule {
        &self.abi
    }
    /// Exact function indices in export declaration order.
    #[must_use]
    pub fn export_functions(&self) -> &[usize] {
        &self.exports
    }
    /// Verified separate operation for one result ID.
    #[must_use]
    pub fn extension(&self, function: usize, result: u32) -> Option<&raw::Operation> {
        self.claim
            .extensions
            .get(function)?
            .iter()
            .find(|e| e.result == result)
            .map(|e| &e.operation)
    }
    /// Complete immutable cleanup plan.
    #[must_use]
    pub fn plan(&self, function: usize) -> Option<&raw::Plan> {
        self.claim.plans.get(function)
    }
}
/// Verifies wire, original opaque bodies, typed CFG and complete owner/loan/drop plans.
/// # Errors
/// Rejects foreign authorities, fabricated effects, moved/loaned use, escapes and cleanup drift.
pub fn verify<'a>(
    decoded: wire::DecodedProgram,
    syntax: &VerifiedProjectSyntaxV5,
    sources: &SourceMap,
    entry: FileId,
    linear: &'a VerifiedLayouts,
    linux: &'a VerifiedLayouts,
    runtime: &'a VerifiedOwnershipRuntimeAbi<'a>,
) -> Result<VerifiedOwnedProgram<'a>, Failure> {
    if !syntax.is_bound_to(sources)
        || sources.verify_file_id(entry.index()).ok() != Some(entry)
        || !runtime.is_bound_to(linear, linux)
    {
        return Err(reject("foreign owned successor authority"));
    }
    let claim = decoded.0;
    let inventory = super::inventory::check(&claim.graph, sources, linear, linux)?;
    let originals = Originals::check(&claim.graph, syntax)?;
    super::copy_v1::entry_closure(&originals, entry.index())?;
    super::substitution::check(&claim.graph, linear, &originals)?;
    super::source_calls::check(&claim.graph, &originals)?;
    source::check(&claim, &originals, sources)?;
    super::source_body::demand(&claim.graph)?;
    super::copy_v1::check_type_demand(&claim.graph, linear)?;
    typed::check(&claim, sources, linear, &inventory)?;
    if plan::derive(&claim.graph, &claim.extensions, linear)? != claim.plans {
        return Err(reject("owned cleanup/loan/failure plan is not exact"));
    }
    let (abi, exports) = super::copy_v1::scalar_abi(&claim.graph, entry.index())?;
    Ok(VerifiedOwnedProgram { claim, linear, linux, runtime, abi, exports })
}
