//! Closed read-only boundary vocabulary for downstream machine planning.
//!
//! Copies of these records cannot construct a source, layout, runtime or program authority.

pub use zryna_semantics::native_c_v0::body::{
    BoundaryCheck, BoundaryDrop, BoundaryExit, BoundaryExitKind, BoundaryOwner, CallEntry,
    FailureRoute, FlowStep, PrivateCopy, PrivateFault, PrivateLoan, PrivateOrigin, PrivateOwner,
    PrivatePreparation, StorageStage, ValueType,
};
pub use zryna_source::{FileId, SourceMapIdentity, Span};
pub use zryna_syntax::native_c_source_v0::raw::{Binding, Statement};
pub use zryna_syntax::native_c_v0::raw::{AbiType, Direction, Mode, Operation, Primitive};
