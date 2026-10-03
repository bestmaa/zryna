//! Immutable backend-facing IR views; none returns mutable records or constructor authority.

use crate::{VerifiedNativeCProgram, raw};
use zryna_layout::{TypeId, VerifiedLayouts};
use zryna_ownership_runtime_abi::VerifiedOwnershipRuntimeAbi;
use zryna_semantics::native_c_v0::body::{
    BoundaryExit, BoundaryOwner, FlowStep, PrivateOwner, PrivatePreparation, ValueType,
    VerifiedPrivateBoundaries,
};
use zryna_source::{FileId, SourceMap, SourceMapIdentity, Span};
use zryna_syntax::native_c_v0::raw as declaration;

impl VerifiedNativeCProgram {
    /// The original retained issuer; a digest is never substituted for it.
    #[must_use]
    pub const fn private_authority(&self) -> &VerifiedPrivateBoundaries {
        &self.authority
    }
    /// Original immutable source identity.
    #[must_use]
    pub fn source_map_identity(&self) -> SourceMapIdentity {
        self.authority.body_authority().source_map_identity()
    }
    /// Checks the actual original map, including when source bytes are equal.
    #[must_use]
    pub fn belongs_to(&self, sources: &SourceMap) -> bool {
        self.authority.belongs_to(sources)
    }
    /// Actual retained native layouts.
    #[must_use]
    pub fn native_layouts(&self) -> &VerifiedLayouts {
        self.authority.native_layouts()
    }
    /// Actual retained Linear32 layouts.
    #[must_use]
    pub fn linear_layouts(&self) -> &VerifiedLayouts {
        self.authority.linear_layouts()
    }
    /// Genuine runtime declaration issuer, without runtime execution capability.
    #[must_use]
    pub fn runtime_abi(&self) -> &VerifiedOwnershipRuntimeAbi {
        self.authority.runtime_abi()
    }
    /// Exact native target requirement, never a backend fallback selector.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.program.declarations.target
    }
    /// Complete imports and exports, including operations unused by source calls.
    #[must_use]
    pub fn operations(&self) -> impl ExactSizeIterator<Item = VerifiedOperation<'_>> {
        self.program
            .declarations
            .operations
            .iter()
            .enumerate()
            .map(|(index, record)| VerifiedOperation { index, record })
    }
    /// Complete original source function inventory.
    #[must_use]
    pub fn functions(&self) -> impl ExactSizeIterator<Item = VerifiedFunction<'_>> {
        self.program.functions.iter().map(|record| VerifiedFunction { record })
    }
}

/// Immutable operation identity and exact source/library/ABI policy.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedOperation<'a> {
    index: usize,
    record: &'a declaration::Operation,
}
impl<'a> VerifiedOperation<'a> {
    /// Canonical declaration ordinal; unused records still consume their bounds.
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }
    /// Read-only exact declaration; copying it cannot mint a program seal.
    #[must_use]
    pub const fn declaration(self) -> &'a declaration::Operation {
        self.record
    }
}

/// Complete immutable native-requirement function.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedFunction<'a> {
    record: &'a raw::Function,
}
impl<'a> VerifiedFunction<'a> {
    /// Original file and declaration ordinal.
    #[must_use]
    pub const fn identity(self) -> (FileId, usize) {
        (self.record.file, self.record.ordinal)
    }
    /// Original declaration location.
    #[must_use]
    pub const fn span(self) -> Span {
        self.record.span
    }
    /// Original source identifier.
    #[must_use]
    pub fn name(self) -> &'a str {
        &self.record.name
    }
    /// Ordered genuine private/scalar input layouts; not a public C signature.
    #[must_use]
    pub fn parameters(self) -> &'a [TypeId] {
        &self.record.parameter_layouts
    }
    /// Genuine result layout and source category.
    #[must_use]
    pub const fn result(self) -> (TypeId, ValueType) {
        (self.record.result_layout, self.record.result)
    }
    /// Only total scalar exports have a public C entry.
    #[must_use]
    pub const fn export(self) -> Option<usize> {
        self.record.export
    }
    /// Complete dense value definitions.
    #[must_use]
    pub fn values(self) -> impl ExactSizeIterator<Item = VerifiedValue<'a>> {
        self.record.values.iter().map(|record| VerifiedValue { record })
    }
    /// Complete operations with explicit private stages and terminal edges.
    #[must_use]
    pub fn effects(self) -> impl ExactSizeIterator<Item = VerifiedEffect<'a>> {
        self.record.effects.iter().map(|record| VerifiedEffect { record })
    }
    /// Private release issuers, kept separate from foreign releases.
    #[must_use]
    pub fn private_owners(self) -> &'a [PrivateOwner] {
        &self.record.private_owners
    }
}

/// One immutable typed value.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedValue<'a> {
    record: &'a raw::Value,
}
impl<'a> VerifiedValue<'a> {
    /// Dense original definition ordinal.
    #[must_use]
    pub const fn id(self) -> usize {
        self.record.id
    }
    /// Original authenticated location.
    #[must_use]
    pub const fn span(self) -> Span {
        self.record.span
    }
    /// Exact typed definition; copying a record supplies no enclosing issuer.
    #[must_use]
    pub const fn definition(self) -> &'a raw::Value {
        self.record
    }
}

/// One immutable effect and its complete exit authority.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedEffect<'a> {
    record: &'a raw::Effect,
}
impl<'a> VerifiedEffect<'a> {
    /// Dense original effect ordinal.
    #[must_use]
    pub const fn id(self) -> usize {
        self.record.id
    }
    /// Closed operation with exact call/status/owner identities.
    #[must_use]
    pub const fn operation(self) -> &'a FlowStep {
        &self.record.operation
    }
    /// Explicit preparation stages and genuine runtime fault issuers.
    #[must_use]
    pub fn preparation(self) -> Option<&'a PrivatePreparation> {
        self.record.preparation.as_ref()
    }
    /// All terminal outcome/cleanup obligations at this program point.
    #[must_use]
    pub fn exits(self) -> &'a [BoundaryExit] {
        &self.record.exits
    }
    /// Conditional successful commitments, never assertions of runtime execution.
    #[must_use]
    pub fn completed(self) -> &'a [BoundaryOwner] {
        &self.record.completed
    }
}
