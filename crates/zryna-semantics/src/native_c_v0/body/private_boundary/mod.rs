//! Isolated private boundary composition; neither an executable entry nor a legacy M3 seal.

use super::{FlowStep, TypedFunction, VerifiedForeignBodies};
use std::fmt;
use zryna_diagnostics::Diagnostic;
use zryna_layout::VerifiedLayouts;
use zryna_ownership_runtime_abi::{RuntimeAbiViolation, VerifiedOwnershipRuntimeAbi};
use zryna_source::{SourceMap, Span};

mod cleanup;
mod copies;
mod layouts;
mod loans;
mod owners;
mod replay;
mod views;

use layouts::LayoutAuthority;
pub use views::{
    BoundaryDrop, BoundaryExit, BoundaryExitKind, BoundaryOwner, BoundaryStep, FunctionBoundary,
    PrivateCopy, PrivateFault, PrivateLoan, PrivateOrigin, PrivateOwner, PrivatePreparation,
    StorageStage,
};

#[cfg(test)]
pub(in crate::native_c_v0::body) use owners::checked_count;

/// Atomic source-bound private boundary plan, with genuine retained declaration issuers.
///
/// This supplies no executable entry, raw IR, runtime allocation, physical cleanup or link seal.
#[derive(Clone, Debug)]
pub struct VerifiedPrivateBoundaries {
    bodies: VerifiedForeignBodies,
    layouts: LayoutAuthority,
    functions: Vec<FunctionBoundary>,
}
impl VerifiedPrivateBoundaries {
    /// Exact original complete body/declaration/material authority.
    #[must_use]
    pub fn body_authority(&self) -> &VerifiedForeignBodies {
        &self.bodies
    }
    /// Genuine independently verified Linear32 private layouts.
    #[must_use]
    pub fn linear_layouts(&self) -> &VerifiedLayouts {
        &self.layouts.linear
    }
    /// Genuine independently verified native Linux private layouts.
    #[must_use]
    pub fn native_layouts(&self) -> &VerifiedLayouts {
        &self.layouts.native
    }
    /// Genuine ownership-runtime declaration authority for those exact layouts.
    #[must_use]
    pub fn runtime_abi(&self) -> &VerifiedOwnershipRuntimeAbi {
        &self.layouts.runtime
    }
    /// Complete read-only private boundary requirements, in original function order.
    #[must_use]
    pub fn functions(&self) -> &[FunctionBoundary] {
        &self.functions
    }
    /// Rejects another source map even when its text is identical.
    #[must_use]
    pub fn belongs_to(&self, sources: &SourceMap) -> bool {
        self.bodies.belongs_to(sources)
    }
}

/// Fail-closed composition rejection, preserving upstream layout/runtime diagnostic evidence.
#[derive(Clone, Debug)]
pub struct BoundaryError {
    code: &'static str,
    detail: &'static str,
    span: Option<Span>,
    layout_diagnostics: Vec<Diagnostic>,
    runtime_violations: Vec<RuntimeAbiViolation>,
}
impl BoundaryError {
    /// Stable native-C producing diagnostic category.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }
    /// Exact rejected requirement.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
    /// Original authenticated source span, when the failure has a source occupant.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        self.span
    }
    /// Unmodified independent layout diagnostics.
    #[must_use]
    pub fn layout_diagnostics(&self) -> &[Diagnostic] {
        &self.layout_diagnostics
    }
    /// Unmodified independent runtime declaration violations.
    #[must_use]
    pub fn runtime_violations(&self) -> &[RuntimeAbiViolation] {
        &self.runtime_violations
    }

    fn new(code: &'static str, detail: &'static str) -> Self {
        Self {
            code,
            detail,
            span: None,
            layout_diagnostics: Vec::new(),
            runtime_violations: Vec::new(),
        }
    }
    fn source(detail: &'static str) -> Self {
        Self::new("ZRYNA-C4106", detail)
    }
    fn owned(detail: &'static str) -> Self {
        Self::new("ZRYNA-C4105", detail)
    }
    fn typed(detail: &'static str) -> Self {
        Self::new("ZRYNA-C4104", detail)
    }
    fn budget(detail: &'static str) -> Self {
        Self::new("ZRYNA-C4107", detail)
    }
    fn layout(diagnostics: Vec<Diagnostic>) -> Self {
        Self { layout_diagnostics: diagnostics, ..Self::source("boundary-layout-verification") }
    }
    fn runtime(violations: Vec<RuntimeAbiViolation>) -> Self {
        Self { runtime_violations: violations, ..Self::source("boundary-runtime-verification") }
    }
    fn at(mut self, bodies: &VerifiedForeignBodies, function: &TypedFunction) -> Self {
        if self.span.is_none() {
            self.span = bodies
                .declaration_authority()
                .syntax
                .span(function.file_id(), function.declaration_range())
                .ok();
        }
        self
    }
}
impl fmt::Display for BoundaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.code, self.detail)
    }
}
impl std::error::Error for BoundaryError {}

/// Derives and independently replays complete private preparation/cleanup requirements.
///
/// Only original source and its opaque complete bodies enter. Public record copies cannot enter
/// this API. Existing layout/runtime verifiers issue the retained private authorities; the old
/// protocol-v4 `SemanticInput`, M3 IR and every executable/native sealer remain separate.
///
/// # Errors
/// Rejects source identity, layout/runtime derivation, compiler budgets or private replay defects.
pub fn compose_private_boundaries(
    sources: &SourceMap,
    bodies: &VerifiedForeignBodies,
) -> Result<VerifiedPrivateBoundaries, BoundaryError> {
    let candidate = build_candidate(sources, bodies)?;
    verify_candidate(sources, bodies, candidate)
}

#[derive(Clone, Debug)]
pub(in crate::native_c_v0::body) struct Candidate {
    pub(in crate::native_c_v0::body) layouts: LayoutAuthority,
    pub(in crate::native_c_v0::body) functions: Vec<FunctionBoundary>,
}

pub(in crate::native_c_v0::body) fn build_candidate(
    sources: &SourceMap,
    bodies: &VerifiedForeignBodies,
) -> Result<Candidate, BoundaryError> {
    if !bodies.belongs_to(sources) {
        return Err(BoundaryError::source("boundary-original-source-map"));
    }
    owners::preflight(bodies.functions())?;
    let layouts = LayoutAuthority::derive(sources)?;
    let functions = bodies
        .functions()
        .iter()
        .map(|function| {
            produce_function(function, &layouts).map_err(|error| error.at(bodies, function))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Candidate { layouts, functions })
}

fn produce_function(
    function: &TypedFunction,
    layouts: &LayoutAuthority,
) -> Result<FunctionBoundary, BoundaryError> {
    let source = owners::derive(function, layouts)?;
    let mut registry = cleanup::Registry::entry(&source);
    let mut steps = Vec::with_capacity(function.steps().len());
    for (index, step) in function.steps().iter().enumerate() {
        let preparation = match step {
            FlowStep::PrepareLoan { .. } => {
                Some(PrivatePreparation::Loan(loans::derive(step, &source, layouts)?))
            }
            FlowStep::Copy { .. } => Some(PrivatePreparation::Copy(copies::derive(step, layouts)?)),
            _ => None,
        };
        let rules = cleanup::rules(
            step,
            preparation.as_ref(),
            cleanup::release_call(function.steps(), index),
        );
        let exits = cleanup::produce(&rules, &registry, &source, cleanup::returned(step, &source))?;
        steps.push(BoundaryStep {
            source_step: index,
            preparation,
            exits,
            completed: cleanup::Registry::completions(step),
        });
        registry.advance(step)?;
    }
    Ok(FunctionBoundary {
        file: function.file_id(),
        source_function: function.source_function_index(),
        parameter_types: function
            .parameters()
            .iter()
            .map(|parameter| layouts.type_id(parameter.ty.into()))
            .collect::<Result<Vec<_>, _>>()?,
        result_type: layouts.type_id(function.result_type())?,
        result_category: function.result_type(),
        expression_origins: source.origins,
        private_owners: source.owners,
        steps,
    })
}

pub(in crate::native_c_v0::body) fn verify_candidate(
    sources: &SourceMap,
    bodies: &VerifiedForeignBodies,
    candidate: Candidate,
) -> Result<VerifiedPrivateBoundaries, BoundaryError> {
    replay::verify(sources, bodies, &candidate)?;
    Ok(VerifiedPrivateBoundaries {
        bodies: bodies.clone(),
        layouts: candidate.layouts,
        functions: candidate.functions,
    })
}
