//! Read-only successor runtime views; no older ABI identity conversions exist.

use super::{Identity, raw};
use zryna_layout::{StorageTarget, generic_v1::TypeId};

/// Exact source- and declaration-bound logical operation identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationId {
    pub(super) owner: Identity,
    pub(super) operation: crate::LogicalOperation,
}

impl OperationId {
    /// Fixed logical operation; this enum alone is not invocation authority.
    #[must_use]
    pub const fn logical(self) -> crate::LogicalOperation {
        self.operation
    }
}

/// Immutable declaration paired with its exact successor issuer identity.
#[derive(Clone, Copy, Debug)]
pub struct OperationView<'a> {
    pub(super) id: OperationId,
    pub(super) declaration: &'a raw::OperationDeclaration,
}

impl<'a> OperationView<'a> {
    /// Exact branded operation identity.
    #[must_use]
    pub const fn id(self) -> OperationId {
        self.id
    }
    /// Exact validated declaration; mutation requires a new verification.
    #[must_use]
    pub const fn declaration(self) -> &'a raw::OperationDeclaration {
        self.declaration
    }
}

/// One verified nonzero Vec element layout in a selected target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElementLayout {
    pub(super) target: StorageTarget,
    pub(super) element: TypeId,
    pub(super) stride: u64,
    pub(super) alignment: u64,
}

impl ElementLayout {
    /// Selected storage target.
    #[must_use]
    pub const fn target(self) -> StorageTarget {
        self.target
    }
    /// Exact successor element identity.
    #[must_use]
    pub const fn element(self) -> TypeId {
        self.element
    }
    /// Verified allocation stride.
    #[must_use]
    pub const fn stride(self) -> u64 {
        self.stride
    }
    /// Verified allocation alignment.
    #[must_use]
    pub const fn alignment(self) -> u64 {
        self.alignment
    }
}

/// One verified control block carrying an exact successor payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlLayout {
    pub(super) target: StorageTarget,
    pub(super) payload: TypeId,
    pub(super) payload_offset: u64,
    pub(super) size: u64,
    pub(super) alignment: u64,
}

impl ControlLayout {
    /// Selected storage target.
    #[must_use]
    pub const fn target(self) -> StorageTarget {
        self.target
    }
    /// Exact successor payload identity.
    #[must_use]
    pub const fn payload(self) -> TypeId {
        self.payload
    }
    /// Verified byte offset of the payload after both counts.
    #[must_use]
    pub const fn payload_offset(self) -> u64 {
        self.payload_offset
    }
    /// Complete padded control allocation size.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }
    /// Complete allocation alignment.
    #[must_use]
    pub const fn alignment(self) -> u64 {
        self.alignment
    }
}
