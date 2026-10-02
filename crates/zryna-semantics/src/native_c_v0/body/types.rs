//! Source types and complete read-only source-derived function/expression views.

use super::FlowStep;
use zryna_source::FileId;
use zryna_syntax::native_c_source_v0::raw as syntax;

/// Independently inferred source value category; nominal identities remain in the sealed plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueType {
    /// Signed language scalar, also used for an explicitly tracked C status.
    I32,
    /// Canonical language Boolean.
    Bool,
    /// Private owned String; concrete M3 entry/layout authority remains a later prerequisite.
    String,
    /// Private owned vector; concrete M3 entry/layout authority remains a later prerequisite.
    VecI32,
    /// Scoped retained private byte loan.
    Bytes,
    /// Caller-owned scalar output storage.
    I32Out,
    /// Caller-owned nominal handle output storage.
    HandleOut,
    /// Caller-owned foreign byte-pointer output storage.
    BytesOut,
    /// Caller-owned count output storage.
    CountOut,
    /// Linear nominal handle value.
    Handle,
    /// Linear nominal foreign byte value, possibly canonically empty at runtime.
    OwnedBytes,
    /// Infallible normal completion of a void call or release.
    Unit,
    /// Terminal guarded foreign outcome, never a scalar value.
    Terminal,
    /// Literal operation/kind key; it cannot become a language String.
    Key,
}

impl From<syntax::Type> for ValueType {
    fn from(value: syntax::Type) -> Self {
        match value {
            syntax::Type::I32 => Self::I32,
            syntax::Type::Bool => Self::Bool,
            syntax::Type::String => Self::String,
            syntax::Type::VecI32 => Self::VecI32,
            syntax::Type::Bytes => Self::Bytes,
            syntax::Type::I32Out => Self::I32Out,
            syntax::Type::HandleOut => Self::HandleOut,
            syntax::Type::BytesOut => Self::BytesOut,
            syntax::Type::CountOut => Self::CountOut,
            syntax::Type::Handle => Self::Handle,
            syntax::Type::OwnedBytes => Self::OwnedBytes,
        }
    }
}

impl ValueType {
    pub(super) const fn boundary(self) -> bool {
        matches!(self, Self::I32 | Self::Bool | Self::String | Self::VecI32)
    }
    pub(super) const fn scalar(self) -> bool {
        matches!(self, Self::I32 | Self::Bool)
    }
    pub(super) const fn linear(self) -> bool {
        matches!(self, Self::String | Self::VecI32 | Self::Handle | Self::OwnedBytes)
    }
}

/// Inferred expression view. Cloning a view cannot construct the enclosing authority.
#[derive(Clone, Debug)]
pub struct TypedExpression {
    pub(super) range: syntax::Range,
    pub(super) ty: ValueType,
    pub(super) token: Option<usize>,
    pub(super) status_call: Option<usize>,
    pub(super) source_kind: syntax::ExpressionKind,
}
impl TypedExpression {
    /// Complete authenticated expression range in the owning function's file.
    #[must_use]
    pub const fn range(&self) -> syntax::Range {
        self.range
    }
    /// Exact inferred source category.
    #[must_use]
    pub const fn value_type(&self) -> ValueType {
        self.ty
    }
    /// Exact status-call provenance, if present, never just an operation name.
    #[must_use]
    pub const fn status_call(&self) -> Option<usize> {
        self.status_call
    }
    /// Exact scoped token identity; it cannot be converted to a C address by this view.
    #[must_use]
    pub const fn token_id(&self) -> Option<usize> {
        self.token
    }
    /// Exact retained source operation; an `Add` requires wrapping i32 lowering.
    /// Copying this view supplies no typed or executable constructor authority.
    #[must_use]
    pub fn source_kind(&self) -> &syntax::ExpressionKind {
        &self.source_kind
    }
}

/// Complete source function with a typed conditional foreign flow plan.
#[derive(Clone, Debug)]
pub struct TypedFunction {
    pub(super) file: FileId,
    pub(super) source_function: usize,
    pub(super) name: String,
    pub(super) result: ValueType,
    pub(super) expressions: Vec<TypedExpression>,
    pub(super) steps: Vec<FlowStep>,
    pub(super) owners: Vec<OwnerOrigin>,
    pub(super) source_range: syntax::Range,
    pub(super) parameters: Vec<syntax::Binding>,
    pub(super) statements: Vec<syntax::Statement>,
    pub(super) export_operation: Option<usize>,
}
impl TypedFunction {
    /// Original immutable source-file identity.
    #[must_use]
    pub const fn file_id(&self) -> FileId {
        self.file
    }
    /// Function ordinal in the independently parsed file.
    #[must_use]
    pub const fn source_function_index(&self) -> usize {
        self.source_function
    }
    /// Exact source identifier.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Independently checked function result category.
    #[must_use]
    pub const fn result_type(&self) -> ValueType {
        self.result
    }
    /// Complete source expression arena, in its original dense postorder.
    #[must_use]
    pub fn expressions(&self) -> &[TypedExpression] {
        &self.expressions
    }
    /// Complete required conditional flow steps, without executable or physical-cleanup proof.
    #[must_use]
    pub fn steps(&self) -> &[FlowStep] {
        &self.steps
    }
    /// Exact call/resource/slot origins of every potential non-null obligation.
    #[must_use]
    pub fn owner_origins(&self) -> &[OwnerOrigin] {
        &self.owners
    }
    /// Complete declaration range in the exact original source file.
    #[must_use]
    pub const fn declaration_range(&self) -> syntax::Range {
        self.source_range
    }
    /// Complete original parameters, whose source types were independently checked.
    #[must_use]
    pub fn parameters(&self) -> &[syntax::Binding] {
        &self.parameters
    }
    /// Complete retained body occupants, referring to the original dense expression arena.
    #[must_use]
    pub fn statements(&self) -> &[syntax::Statement] {
        &self.statements
    }
    /// Canonical verified export ordinal, present only for an authenticated total scalar export.
    #[must_use]
    pub const fn export_operation_index(&self) -> Option<usize> {
        self.export_operation
    }
}

/// Read-only nominal origin retained even when an output was never taken by source.
#[derive(Clone, Debug)]
pub struct OwnerOrigin {
    pub(super) inner: super::state::Owner,
}
impl OwnerOrigin {
    /// Exact creating raw call ordinal, independent of a status variable spelling.
    #[must_use]
    pub const fn creating_call(&self) -> usize {
        self.inner.call
    }
    /// Exact verified resource-group ordinal within that operation.
    #[must_use]
    pub const fn resource_group(&self) -> usize {
        self.inner.group
    }
    /// Complete exact output-token pair or handle slot, in declared ABI order.
    #[must_use]
    pub fn output_slots(&self) -> &[usize] {
        &self.inner.slots
    }
    /// Exact nominal library identity.
    #[must_use]
    pub fn library(&self) -> &str {
        &self.inner.library
    }
    /// Exact nominal foreign kind, never derived from a pointer width.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.inner.kind
    }
    /// Exact allocator identity authenticated with the creating and releasing declarations.
    #[must_use]
    pub fn allocator(&self) -> &str {
        &self.inner.allocator
    }
    /// Exact matching release identity.
    #[must_use]
    pub fn release_key(&self) -> &str {
        &self.inner.release
    }
}
