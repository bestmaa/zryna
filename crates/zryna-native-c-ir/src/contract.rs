//! Closed read-only boundary vocabulary for downstream machine planning.
//!
//! Copies of these records cannot construct a source, layout, runtime or program authority.

pub use crate::raw::{Value, ValueKind};
pub use zryna_semantics::native_c_v0::body::{
    BoundaryCheck, BoundaryDrop, BoundaryExit, BoundaryExitKind, BoundaryOwner, CallEntry,
    CleanupEntry, FailureRoute, FlowStep, PrivateCopy, PrivateFault, PrivateLoan, PrivateOrigin,
    PrivateOwner, PrivatePreparation, StorageStage, TrapRequirement, ValueType,
};
pub use zryna_source::{FileId, SourceMapIdentity, Span};
pub use zryna_syntax::native_c_source_v0::raw::{
    Binding, Statement, StatementKind, Type as SourceType,
};
pub use zryna_syntax::native_c_v0::raw::{
    AbiType, Access, Category, Direction, Encoding, Mode, NullRule, Operation, Primitive, Safety,
};
