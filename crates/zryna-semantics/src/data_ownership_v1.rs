//! Aggregate semantic lowering for the isolated `DataOwnershipV1` profile.
//!
//! This boundary accepts only authenticated protocol-v4 syntax, derives both layout authorities
//! itself, and returns only verifier-sealed IR. Raw layout and IR claims never cross the API.

use zryna_diagnostics::{Diagnostic, Severity};
use zryna_ir::data_ownership_v1::{self as ir, RuntimeContractIdentity, raw};
use zryna_layout::{self as layout, StorageTarget, TypeCategory, raw as raw_layout};
use zryna_ownership_runtime_abi as ownership_runtime_abi;
use zryna_source::{FileId, MAX_SOURCE_FILES, SourceMap, Span};
use zryna_syntax::v4::{
    self as syntax, RawDataDeclarationKind, RawExpressionKind, RawStatementKind,
};

mod aggregate_resource_formulas;
mod borrow_call_preflight;
mod borrow_call_resources;
mod borrow_forwarding;
pub(crate) mod command_support;
mod copy_enum_match;
mod copy_function_lowering;
mod copy_lowering;
mod diagnostics;
mod function_catalog;
mod function_dispatch;
mod global_resource_limits;
mod import_resolution;
mod layout_graph;
mod lowering;
mod owned_aggregate_lowering;
mod owned_call_resolution;
mod owned_cfg_state;
mod owned_constructor_plan;
mod owned_control_flow_resources;
mod owned_control_flow_shape;
mod owned_enum_payload_move;
mod owned_lowering_resources;
mod owned_root_borrow_planning;
mod owned_root_borrow_postprocessing;
mod owned_string_lowering;
mod owned_string_read;
mod owned_vec_lowering;
mod owner_state;
mod root_borrow_arm_planning;
mod root_borrow_call_planning;
mod root_borrow_execution;
mod root_borrow_function_lowering;
mod root_borrow_shape_planning;
mod root_borrow_straight_planning;
mod root_borrow_value_planning;
mod scalar_operations;
mod string_vec_resource_estimates;
mod type_model;

#[cfg(test)]
use aggregate_resource_formulas::{
    PartialTransferBudgetViolation, aggregate_clone_budget_violation,
    partial_assignment_budget_preflight, partial_return_budget_preflight,
    partial_transfer_budget_preflight, projected_aggregate_clone_budget_violation,
    projected_string_clone_budget_violation,
};
#[cfg(test)]
use aggregate_resource_formulas::{
    projected_aggregate_assignment_budget_violation,
    projected_aggregate_clone_assignment_budget_violation,
    projected_subobject_assignment_budget_violation,
};
#[cfg(test)]
use aggregate_resource_formulas::{
    projected_subobject_move_budget_violation, projected_subobject_return_budget_violation,
};
use borrow_call_resources::preflight_program_borrow_calls;
use copy_lowering::{BorrowBinding, FunctionLowerer};
use diagnostics::{Errors, span};
use function_catalog::{FunctionCatalog, build_function_catalog};
#[cfg(test)]
use function_catalog::{FunctionParameterOrder, FunctionSignature};
use function_dispatch::lower_function;
#[cfg(test)]
use global_resource_limits::checked_string_concat_bytes;
use global_resource_limits::{
    accumulate_generated_cfg_function, accumulate_generated_value_function, semantic_preflight,
};
#[cfg(test)]
use global_resource_limits::{
    aggregate_operand_budget_violation, aggregate_transition_budget_violation,
};
use layout_graph::{Decl, build_graph, semantic_type};
#[cfg(test)]
use owned_control_flow_resources::enum_payload_move_resource_violation;
#[cfg(test)]
use owned_control_flow_resources::{
    preflight_owned_place_capacity, preflight_owned_place_capacity_with_reserved,
};
#[cfg(test)]
use owned_control_flow_shape::{preflight_owned_loop_body, preflight_owned_loop_exit};
#[cfg(test)]
use owned_lowering_resources::{OwnedCleanupAccounting, OwnedCleanupActionContext};
#[cfg(test)]
use owned_root_borrow_planning::is_direct_owned_root_borrow_candidate;
use owner_state::OwnerState;
#[cfg(test)]
use string_vec_resource_estimates::{
    OwnedStringEstimateContext, OwnedStringPreparationEstimate, cleanup_actions_after_additions,
    cleanup_actions_after_preparation, cleanup_actions_after_transfer,
    estimate_owned_string_expression, vec_push_target_invalid,
};
use type_model::{
    Binding, OwnedProjectionShapeEntry, OwnedStaticProjectionKind, Ty, map_node_types,
};

/// Maximum retained semantic diagnostics, including the terminal budget diagnostic.
pub const MAX_SEMANTIC_DIAGNOSTICS: usize = 256;

const _: () = {
    assert!(MAX_SOURCE_FILES <= ir::MAX_MODULES);
    assert!(syntax::MAX_FUNCTIONS_PER_MODULE <= ir::MAX_FUNCTIONS_PER_MODULE);
    assert!(syntax::MAX_FUNCTIONS_PER_PROJECT <= ir::MAX_FUNCTIONS_PER_PROGRAM);
    assert!(syntax::MAX_PARAMETERS_PER_FUNCTION <= ir::MAX_PARAMETERS_PER_FUNCTION);
    assert!(syntax::MAX_DATA_DECLARATIONS_PER_MODULE <= ir::MAX_NOMINAL_DECLARATIONS);
    assert!(syntax::MAX_AGGREGATE_OPERANDS_PER_PROJECT <= ir::MAX_AGGREGATE_OPERANDS);
    assert!(MAX_SEMANTIC_DIAGNOSTICS == ir::MAX_DIAGNOSTICS);
};

/// Exact authenticated inputs for aggregate M3 semantics.
///
/// Raw protocol claims cannot enter this boundary.
///
/// ```compile_fail
/// fn bypass<'a>(raw: &'a zryna_syntax::v4::RawProjectSyntaxSnapshot,
///     sources: &'a zryna_source::SourceMap, entry: zryna_source::FileId)
///     -> Option<zryna_semantics::data_ownership_v1::SemanticInput<'a>> {
///     zryna_semantics::data_ownership_v1::SemanticInput::try_new(raw, sources, entry)
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct SemanticInput<'a> {
    syntax: &'a syntax::ProjectSyntaxSnapshot,
    sources: &'a SourceMap,
    entry: FileId,
    command: Option<&'a zryna_syntax::command_h1_v1::CommandSyntax>,
}

impl<'a> SemanticInput<'a> {
    /// Authenticates syntax, source authority, provider success, and the exact entry module.
    #[must_use]
    pub fn try_new(
        syntax: &'a syntax::ProjectSyntaxSnapshot,
        sources: &'a SourceMap,
        entry: FileId,
    ) -> Option<Self> {
        (syntax.is_bound_to(sources)
            && sources.source(entry).is_some()
            && syntax.files().iter().filter(|file| file.id() == entry).count() == 1
            && syntax.diagnostics().iter().all(|d| d.severity() != Severity::Error))
        .then_some(Self { syntax, sources, entry, command: None })
    }

    /// Returns the authenticated syntax snapshot.
    #[must_use]
    pub const fn syntax(self) -> &'a syntax::ProjectSyntaxSnapshot {
        self.syntax
    }
    /// Returns the authoritative source map.
    #[must_use]
    pub const fn sources(self) -> &'a SourceMap {
        self.sources
    }
    /// Returns the independently selected entry source.
    #[must_use]
    pub const fn entry(self) -> FileId {
        self.entry
    }
}

/// Sealed M3 semantic result retaining verified IR and its exact ownership-runtime ABI authority.
///
/// Raw IR and runtime declarations cannot be recovered through this boundary.
///
/// ```compile_fail
/// fn recover(program: &zryna_semantics::data_ownership_v1::VerifiedProgram) {
///     let _: &zryna_ir::data_ownership_v1::raw::Program = program.verified_ir().raw();
/// }
/// ```
#[derive(Clone, Debug)]
pub struct VerifiedProgram {
    ir: ir::VerifiedProgram,
    runtime_abi: ownership_runtime_abi::VerifiedOwnershipRuntimeAbi,
}

impl VerifiedProgram {
    /// Returns the opaque verified IR modules.
    #[must_use]
    pub fn modules(&self) -> impl ExactSizeIterator<Item = ir::VerifiedModule<'_>> {
        self.ir.modules()
    }
    /// Returns the retained exact ownership-runtime declaration authority.
    #[must_use]
    pub const fn runtime_abi(&self) -> &ownership_runtime_abi::VerifiedOwnershipRuntimeAbi {
        &self.runtime_abi
    }
    /// Returns the underlying sealed IR authority without exposing raw claims.
    #[must_use]
    pub const fn verified_ir(&self) -> &ir::VerifiedProgram {
        &self.ir
    }
}

/// Successful M3 lowering always carries mandatory IR and runtime-ABI verifier authority.
pub type SemanticResult = Result<VerifiedProgram, Vec<Diagnostic>>;

/// Resolves Copy-only aggregate semantics, derives dual layouts, lowers raw IR deterministically,
/// and immediately invokes the mandatory ownership verifier.
///
/// # Errors
/// Returns stable, bounded, source-located M3 semantic diagnostics, layout diagnostics, or IR
/// verifier diagnostics. No partially checked artifact is returned.
pub fn lower(input: SemanticInput<'_>) -> SemanticResult {
    let candidate = lowering::candidate(input)?;
    let verified_ir = ir::verify(
        candidate.program,
        input.sources(),
        input.entry(),
        candidate.linear,
        candidate.linux,
    )?;
    Ok(VerifiedProgram { ir: verified_ir, runtime_abi: candidate.runtime_abi })
}
#[cfg(test)]
fn authenticated_type_capabilities(
    input: SemanticInput<'_>,
    module: usize,
    type_syntax: u32,
) -> Result<Ty, Vec<Diagnostic>> {
    let mut errors = Errors::new(input.sources());
    semantic_preflight(input, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let (graph, declarations) = build_graph(input, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let linear = layout::verify(&graph, input.sources(), StorageTarget::Linear32V1)?;
    let linux = layout::verify(&graph, input.sources(), StorageTarget::LinuxX8664V1)?;
    if linear.universe_identity() != linux.universe_identity() {
        errors.global(
            "ZRYNA-M3004",
            "the independently derived layout universes disagree",
            "reduce the owned type graph and report this deterministic compiler failure",
        );
        return Err(errors.finish());
    }
    let node_types = map_node_types(&graph, &linear, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let Some(file) = input.syntax().files().get(module) else {
        errors.global(
            "ZRYNA-M3002",
            "the requested type module is outside the authenticated project",
            "resolve types only inside one authenticated source module",
        );
        return Err(errors.finish());
    };
    semantic_type(file, type_syntax, module, &declarations, &graph, &node_types, &mut errors)
        .ok_or_else(|| errors.finish())
}

#[cfg(test)]
mod tests;
