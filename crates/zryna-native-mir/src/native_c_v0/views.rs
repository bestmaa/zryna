//! Immutable machine views; no mutable records, raw program recovery or issuer constructors.

use super::{
    VerifiedMirProgram,
    abi::{OutputSlot, Signature},
    raw,
};
use zryna_layout::TypeId;
use zryna_native_c_ir::{
    VerifiedNativeCProgram,
    contract::{
        Binding, BoundaryExit, BoundaryOwner, FileId, FlowStep, Operation, PrivateOwner,
        PrivatePreparation, Span, Statement, ValueType,
    },
    raw::Value,
};

impl VerifiedMirProgram {
    /// Exact retained original IR authority, including actual source/material/layout issuers.
    #[must_use]
    pub const fn source(&self) -> &VerifiedNativeCProgram {
        &self.source
    }
    /// Exact target requirement; no host fallback selector.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.program.target
    }
    /// Exact hidden checked dispatcher, without raw context or invocation constructor authority.
    #[must_use]
    pub const fn dispatcher(&self) -> &super::entry::DispatcherEntry {
        &self.program.dispatcher
    }
    /// Complete declarations, including unused imports, with verified physical signatures.
    #[must_use]
    pub fn operations(&self) -> impl ExactSizeIterator<Item = VerifiedOperation<'_>> {
        self.program.operations.iter().map(|record| VerifiedOperation { record })
    }
    /// Complete original source functions with verified machine plans.
    #[must_use]
    pub fn functions(&self) -> impl ExactSizeIterator<Item = VerifiedFunction<'_>> {
        self.program.functions.iter().map(|record| VerifiedFunction { record })
    }
}
/// Exact immutable C declaration and physical call signature.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedOperation<'a> {
    record: &'a raw::OperationPlan,
}
impl<'a> VerifiedOperation<'a> {
    /// Dense declaration identity.
    #[must_use]
    pub const fn index(self) -> usize {
        self.record.index
    }
    /// Exact retained library/symbol/policy record.
    #[must_use]
    pub const fn declaration(self) -> &'a Operation {
        &self.record.declaration
    }
    /// Ordered INTEGER lanes, result width and sixteen-byte outgoing stack alignment.
    #[must_use]
    pub const fn signature(self) -> &'a Signature {
        &self.record.signature
    }
}
/// Immutable complete original function and its independent machine proof.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedFunction<'a> {
    record: &'a raw::Function,
}
impl<'a> VerifiedFunction<'a> {
    /// Original file/declaration identity.
    #[must_use]
    pub const fn identity(self) -> (FileId, usize) {
        (self.record.file, self.record.ordinal)
    }
    /// Exact original identifier.
    #[must_use]
    pub fn name(self) -> &'a str {
        &self.record.name
    }
    /// Source declaration location.
    #[must_use]
    pub const fn span(self) -> Span {
        self.record.span
    }
    /// Complete lexical bindings and parameter layouts, separate from public C signatures.
    #[must_use]
    pub fn bindings(self) -> &'a [Binding] {
        &self.record.bindings
    }
    /// Source-ordered layout identities.
    #[must_use]
    pub fn parameters(self) -> &'a [TypeId] {
        &self.record.parameters
    }
    /// Exact result category and private layout.
    #[must_use]
    pub const fn result(self) -> (TypeId, ValueType) {
        self.record.result
    }
    /// Total scalar public export ordinal; other functions require a private entry.
    #[must_use]
    pub const fn export(self) -> Option<usize> {
        self.record.export
    }
    /// Fixed compiler-private checked entry, separate from the source's public C ABI.
    #[must_use]
    pub const fn entry(self) -> &'a super::entry::PrivateEntry {
        &self.record.entry
    }
    /// Exact original statement inventory.
    #[must_use]
    pub fn statements(self) -> &'a [Statement] {
        &self.record.statements
    }
    /// Complete dense source values, including status and private-origin provenance.
    #[must_use]
    pub fn values(self) -> &'a [Value] {
        &self.record.values
    }
    /// Independent exact private release requirements.
    #[must_use]
    pub fn private_owners(self) -> &'a [PrivateOwner] {
        &self.record.private_owners
    }
    /// Non-overlapping, aligned zeroed caller storage; logical initialization follows status zero.
    #[must_use]
    pub fn slots(self) -> &'a [OutputSlot] {
        &self.record.slots
    }
    /// Exact padded caller-output area size.
    #[must_use]
    pub const fn output_frame_bytes(self) -> u32 {
        self.record.output_frame_bytes
    }
    /// Complete independently admitted effect sequence.
    #[must_use]
    pub fn effects(self) -> impl ExactSizeIterator<Item = VerifiedEffect<'a>> {
        self.record.effects.iter().map(|record| VerifiedEffect { record })
    }
}
/// One immutable source effect with exact machine order and terminal cleanup actions.
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
    /// Exact source-bound conditional operation.
    #[must_use]
    pub const fn operation(self) -> &'a FlowStep {
        &self.record.operation
    }
    /// Genuine private stride/preparation/fault plan.
    #[must_use]
    pub fn preparation(self) -> Option<&'a PrivatePreparation> {
        self.record.preparation.as_ref()
    }
    /// Closed ordered machine instructions.
    #[must_use]
    pub fn instructions(self) -> &'a [raw::Instruction] {
        &self.record.instructions
    }
    /// Complete terminal domains and unresolved/protected obligations.
    #[must_use]
    pub fn exits(self) -> &'a [BoundaryExit] {
        &self.record.exits
    }
    /// Exact reverse cleanup action list for each terminal boundary.
    #[must_use]
    pub fn exit_instructions(self) -> &'a [Vec<raw::ExitInstruction>] {
        &self.record.exit_instructions
    }
    /// Conditional owners appended only on successful effect completion.
    #[must_use]
    pub fn completed(self) -> &'a [BoundaryOwner] {
        &self.record.completed
    }
}
