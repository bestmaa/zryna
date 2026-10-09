//! Read-only requirement records; constructing a record grants no enclosing authority.

use super::super::{BoundaryCheck, CleanupEntry, FailureRoute, TrapRequirement, ValueType};
use zryna_layout::TypeId;
use zryna_ownership_runtime_abi::{
    OperationIdentity, OwnershipRuntimeAbiIdentity, VerifiedStatusDeclaration,
};
use zryna_source::FileId;

/// Exact function-local source origin of private storage.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PrivateOrigin {
    /// Original source parameter ordinal.
    Parameter(usize),
    /// Original byte-copy expression ordinal.
    Copy(usize),
    /// Original packed-borrow expression ordinal.
    Packed(usize),
}

/// A conditional completed owner, without an address or runtime liveness assertion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundaryOwner {
    /// Private parameter, preparation or copy origin.
    Private(PrivateOrigin),
    /// Exact foreign-body obligation ordinal.
    Foreign(usize),
}

/// Exact existing private release operation and storage requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateOwner {
    /// Stable original source origin, preserved across moves.
    pub origin: PrivateOrigin,
    /// Existing stored type; packed scratch has no source/layout type.
    pub ty: Option<TypeId>,
    /// Genuine private release operation, whose statuses are only zero and ABI failure.
    pub release: OperationIdentity,
    /// Dynamic nonempty storage gates physical release; empty logical owners still move.
    pub nonempty_storage_only: bool,
    /// Packed allocation size comes from this exact loan's checked byte length.
    pub packed_size_from: Option<usize>,
    /// Exact raw-release alignment for packed allocation; absent for typed handle release.
    pub packed_alignment: Option<u32>,
}

/// Private controlled trap issued by the retained runtime declaration authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrivateFault {
    /// Retained ownership-runtime issuer.
    pub runtime: OwnershipRuntimeAbiIdentity,
    /// Exact fallible preparation operation from that issuer.
    pub operation: OperationIdentity,
    /// Authenticated status disposition and existing trap identity.
    pub declaration: VerifiedStatusDeclaration,
}

/// Ordered byte-backing checks; no item asserts that memory has been inspected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageStage {
    /// Validate paired foreign pointer/count policy before any byte read.
    ValidateForeign,
    /// Complete wrapper length check before storage preparation or C entry.
    Length,
    /// Every source i32 lies in the inclusive unsigned-byte range.
    ByteRange,
    /// Complete initialized private String bytes are valid UTF-8.
    Utf8,
    /// Checked multiplication by the genuine i32 element stride.
    CheckedCapacity,
    /// Allocate only for nonempty backing; preserve failure-atomic out storage.
    AllocateNonempty,
    /// Initialize exactly the byte length, without storage reinterpretation.
    Initialize,
    /// Commit the fully initialized result or scratch only after preparation succeeds.
    Commit,
}

/// Scoped loan with retained private storage and an explicit conversion contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateLoan {
    /// Original preparation expression.
    pub expression: usize,
    /// Original scoped token, shared by its source aliases.
    pub token: usize,
    /// Retained source owner origin, independent of the current binding name.
    pub source: PrivateOrigin,
    /// Genuine private source layout.
    pub source_type: TypeId,
    /// Packed scratch owner; absent for the direct private String byte loan.
    pub scratch: Option<PrivateOrigin>,
    /// Exact existing allocation operation, absent for String retention.
    pub allocation: Option<OperationIdentity>,
    /// Private allocation/capacity faults, absent for an infallible String loan.
    pub faults: Vec<PrivateFault>,
    /// Required preparation order, independently checked before sealing.
    pub stages: Vec<StorageStage>,
    /// Fixed wrapper byte ceiling.
    pub maximum_bytes: usize,
    /// Private source element stride: four for i32, one for String bytes.
    pub source_stride: u64,
    /// C backing stride, always one byte.
    pub backing_stride: u64,
    /// Byte scratch or String byte storage alignment; no i32 pointer cast supplies it.
    pub backing_alignment: u64,
    /// Linux native pointer and `size_t` lane width.
    pub native_bits: u8,
    /// Zero length requires null/zero backing without acquiring scratch storage.
    pub empty_without_allocation: bool,
}

/// Foreign-byte expansion into a distinct initialized private `Vec<i32>`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateCopy {
    /// Original copy expression.
    pub expression: usize,
    /// Exact validated foreign owner retained throughout preparation.
    pub foreign_owner: usize,
    /// Distinct private result origin.
    pub result: PrivateOrigin,
    /// Genuine private vector type and its exact element type.
    pub vector_type: TypeId,
    /// Genuine native i32 element type.
    pub element_type: TypeId,
    /// Verified native element stride and alignment.
    pub stride: u64,
    /// Verified native element alignment.
    pub alignment: u64,
    /// Genuine `VecAllocate` operation.
    pub allocation: OperationIdentity,
    /// Issued allocation/capacity faults; foreign statuses cannot substitute.
    pub faults: Vec<PrivateFault>,
    /// Required expansion, initialized-prefix and length-commit order.
    pub stages: Vec<StorageStage>,
    /// Each unsigned byte becomes one nonnegative i32, never adopted storage.
    pub zero_extend_bytes: bool,
    /// Canonical empty copy has no allocation obligation.
    pub empty_without_allocation: bool,
}

/// Conditional drop from the unified successful-completion order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundaryDrop {
    /// Private runtime release under its original issuer.
    Private(PrivateOwner),
    /// Exact conditional foreign release/validation requirement.
    Foreign(CleanupEntry),
}
impl BoundaryDrop {
    /// Exact obligation that must remain unresolved if this release fails.
    #[must_use]
    pub fn owner(&self) -> BoundaryOwner {
        match self {
            Self::Private(owner) => BoundaryOwner::Private(owner.origin),
            Self::Foreign(owner) => BoundaryOwner::Foreign(owner.owner_id()),
        }
    }
}

/// Distinct exit domains, without scalar reinterpretation or execution evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundaryExitKind {
    /// Existing specified foreign controlled trap.
    ForeignTrap(TrapRequirement),
    /// Genuine private preparation controlled trap.
    PrivateTrap(PrivateFault),
    /// Private operation status 255; it has no controlled release trap.
    PrivateAbiFailure(OperationIdentity),
    /// Exact retained foreign-body failure route.
    ForeignFailure(FailureRoute),
    /// Exact call carrier/pointer/count check failure, classified as host/ABI failure.
    ForeignBoundaryFailure(BoundaryCheck),
    /// Prepared normal result; transfer follows successful cleanup.
    Return,
}

/// Required reverse cleanup and unresolved ownership for one conditional exit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryExit {
    /// Exact exit domain and issuer.
    pub kind: BoundaryExitKind,
    /// Reverse scoped loan ends, all preceding owner drops.
    pub end_loans: Vec<usize>,
    /// Reverse successful-completion prefix, with conditional foreign entries.
    pub cleanup: Vec<BoundaryDrop>,
    /// Obligations that this exit cannot safely release.
    pub unresolved: Vec<BoundaryOwner>,
    /// Prepared private return owner, never transferred before cleanup confirms.
    pub protected_result: Option<PrivateOrigin>,
    /// False only for process failure, where physical cleanup cannot be promised.
    pub cleanup_required: bool,
    /// A private preparation status can occur only when nonempty allocation was attempted.
    pub preparation_nonempty_only: bool,
    /// Any attempted cleanup release failure overrides this exit without retry or result transfer.
    pub release_failure_route: FailureRoute,
}
impl BoundaryExit {
    /// Failed and remaining releases plus unresolved/protected owners; never retries.
    #[must_use]
    pub fn unresolved_after_release_failure(&self, index: usize) -> Option<Vec<BoundaryOwner>> {
        if index >= self.cleanup.len() {
            return None;
        }
        let remaining = self.cleanup.get(index..)?;
        let mut owners = remaining.iter().map(BoundaryDrop::owner).collect::<Vec<_>>();
        owners.extend_from_slice(&self.unresolved);
        if let Some(result) = self.protected_result {
            owners.push(BoundaryOwner::Private(result));
        }
        Some(owners)
    }
}

/// Private preparation attached to an exact retained foreign flow step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivatePreparation {
    /// Scoped packed bytes or retained initialized String bytes.
    Loan(PrivateLoan),
    /// Distinct private `Vec<i32>` byte expansion.
    Copy(PrivateCopy),
}

/// Complete composition of one original foreign flow step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryStep {
    /// Dense original foreign flow-step ordinal; no step may be projected away.
    pub source_step: usize,
    /// Private preparation required by this exact source step, if any.
    pub preparation: Option<PrivatePreparation>,
    /// Required conditional exits before this step's owner commitment.
    pub exits: Vec<BoundaryExit>,
    /// Successfully completed conditional owners appended by this step.
    pub completed: Vec<BoundaryOwner>,
}

/// Complete function-local private boundary requirements, without executable entry authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionBoundary {
    /// Original source file authority.
    pub file: FileId,
    /// Original function ordinal within that file.
    pub source_function: usize,
    /// Source-ordered existing private/scalar parameter layout identities.
    pub parameter_types: Vec<TypeId>,
    /// Exact source result layout and category.
    pub result_type: TypeId,
    /// Exact source result category.
    pub result_category: ValueType,
    /// All source expression owner origins, including private move references.
    pub expression_origins: Vec<Option<PrivateOrigin>>,
    /// Complete private owners in successful preparation order.
    pub private_owners: Vec<PrivateOwner>,
    /// Complete dense composition of the retained foreign steps.
    pub steps: Vec<BoundaryStep>,
}
