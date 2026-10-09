//! Source-derived typed foreign bodies and conditional ownership/cleanup plans.
//!
//! This authority is separate from every existing M3, IR, MIR and executable seal. It consumes
//! only the complete source syntax retained by verified declarations. Runtime implementations
//! must still independently prove ABI checks, bounded acquisition, private preparation, actual
//! cleanup and native linking; a conditional plan is not execution evidence.

use std::{collections::BTreeMap, fmt};
use zryna_source::{FileId, SourceMap, SourceMapIdentity, Span};
use zryna_syntax::native_c_source_v0::raw as syntax;

use super::VerifiedDeclarationSet;

mod acquisitions;
mod arguments;
mod bindings;
mod calls;
mod cleanup;
mod expressions;
mod flow;
mod outcomes;
mod primitives;
mod private_boundary;
mod slots;
mod state;
mod status;
mod tokens;
mod types;

pub use cleanup::CleanupEntry;
pub use flow::FlowStep;
pub use outcomes::{BoundaryCheck, CallEntry, FailureRoute, TrapRequirement};
pub use private_boundary::{
    BoundaryDrop, BoundaryError, BoundaryExit, BoundaryExitKind, BoundaryOwner, BoundaryStep,
    FunctionBoundary, PrivateCopy, PrivateFault, PrivateLoan, PrivateOrigin, PrivateOwner,
    PrivatePreparation, StorageStage, VerifiedPrivateBoundaries, compose_private_boundaries,
};
use state::{Binding, Call, Owner, Token, Value};
pub use types::{OwnerOrigin, TypedExpression, TypedFunction, ValueType};

/// Fixed foreign execution-instance limit; reservations are not acquired owners.
pub const MAX_LIVE_FOREIGN_OBLIGATIONS: usize = 64;

/// Opaque complete typed foreign-body authority bound to the original declaration/source set.
#[derive(Clone, Debug)]
pub struct VerifiedForeignBodies {
    declarations: VerifiedDeclarationSet,
    functions: Vec<TypedFunction>,
}
impl VerifiedForeignBodies {
    /// Exact original immutable source map.
    #[must_use]
    pub fn source_map_identity(&self) -> SourceMapIdentity {
        self.declarations.source_map_identity()
    }
    /// Exact canonical declaration digest retained by this authority.
    #[must_use]
    pub fn declaration_sha256(&self) -> &[u8; 32] {
        self.declarations.declaration_sha256()
    }
    /// Complete typed source functions. A view never supplies constructor authority.
    #[must_use]
    pub fn functions(&self) -> &[TypedFunction] {
        &self.functions
    }
    /// Direct retained declaration/material authority, never reconstructed from a digest.
    #[must_use]
    pub fn declaration_authority(&self) -> &VerifiedDeclarationSet {
        &self.declarations
    }
    /// Rejects a rebuilt map even when every byte matches.
    #[must_use]
    pub fn belongs_to(&self, sources: &SourceMap) -> bool {
        self.declarations.belongs_to(sources)
    }
}

/// One fail-closed typed-body rejection, with an authenticated source location when available.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BodyError {
    code: &'static str,
    detail: &'static str,
    span: Option<Span>,
}
impl BodyError {
    /// Fixed producing native-C diagnostic category.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }
    /// Exact rejected type/flow category.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
    /// Location issued by the retained original map; absent for global identity failures.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        self.span
    }
}
impl fmt::Display for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.code, self.detail)
    }
}
impl std::error::Error for BodyError {}

/// Independently checks every complete body retained by declaration authentication.
///
/// No caller AST, typed node, ownership ledger, cleanup plan, provider status or raw IR enters.
/// The result is atomic and source-bound. It remains a plan for the foreign extension, without
/// granting private M3 execution, C trust, runtime cleanup, IR/MIR or link authority.
///
/// # Errors
/// Rejects foreign source identity, incomplete bodies, wrong types and unsafe token/status flow.
pub fn verify_bodies(
    sources: &SourceMap,
    declarations: &VerifiedDeclarationSet,
) -> Result<VerifiedForeignBodies, BodyError> {
    if !declarations.belongs_to(sources) {
        return Err(BodyError {
            code: "ZRYNA-C4106",
            detail: "body-source-map-identity",
            span: None,
        });
    }
    let mut functions = Vec::new();
    for file in declarations.syntax.files() {
        for (ordinal, function) in file.functions().iter().enumerate() {
            functions.push(Frame::new(declarations, file.file_id(), function)?.check(ordinal)?);
        }
    }
    Ok(VerifiedForeignBodies { declarations: declarations.clone(), functions })
}

struct Frame<'a> {
    declarations: &'a VerifiedDeclarationSet,
    file: FileId,
    function: &'a syntax::Function,
    bindings: Vec<Binding>,
    names: BTreeMap<String, usize>,
    tokens: Vec<Token>,
    calls: Vec<Call>,
    owners: Vec<Owner>,
    typed: Vec<Option<TypedExpression>>,
    steps: Vec<FlowStep>,
    guard_call: Option<usize>,
}
impl Frame<'_> {
    fn error(&self, range: syntax::Range, code: &'static str, detail: &'static str) -> BodyError {
        BodyError { code, detail, span: self.declarations.syntax.span(self.file, range).ok() }
    }
    fn resource_error(&self, range: syntax::Range, detail: &'static str) -> BodyError {
        self.error(range, "ZRYNA-C4105", detail)
    }
    fn type_error(&self, range: syntax::Range, detail: &'static str) -> BodyError {
        self.error(range, "ZRYNA-C4104", detail)
    }
    fn source_error(&self, range: syntax::Range, detail: &'static str) -> BodyError {
        self.error(range, "ZRYNA-C4106", detail)
    }
}

#[cfg(test)]
mod tests;
