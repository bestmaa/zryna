//! Compiler-private ownership-runtime ABI declarations.
//!
//! Raw values are untrusted claims. [`verify_v1`] is the only constructor of the opaque verified
//! authority. This crate declares no executable runtime and exposes no public aggregate host ABI.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};
use zryna_layout::{StorageTarget, TypeCategory, TypeId, TypeUniverseIdentity, VerifiedLayouts};

mod declarations;
use declarations::canonical_contract;
pub mod generic_v1;

pub mod control_model;
pub use control_model::ControlState;

/// Exact frozen ABI identifier.
pub const OWNERSHIP_RUNTIME_V1_IDENTIFIER: &str = "zryna-ownership-runtime-v1";
/// Exact serialized declaration schema version.
pub const OWNERSHIP_RUNTIME_V1_SCHEMA_VERSION: u32 = 1;
/// Maximum untrusted operation declarations admitted before exact inventory verification.
pub const MAX_RUNTIME_OPERATIONS: usize = 256;
/// Maximum target functions or symbols admitted by runtime ABI verification.
pub const MAX_TARGET_FUNCTIONS: usize = 4_096;
/// Maximum sealed layout references admitted by runtime ABI verification.
pub const MAX_LAYOUT_REFERENCES: usize = 65_536;
/// Maximum nested parameter, status, result-lane, and record-field declarations.
pub const MAX_DECLARATION_CHILDREN: usize = 65_536;
/// Maximum bytes in checked repository header evidence.
pub const MAX_CHECKED_HEADER_BYTES: usize = 16 * 1024 * 1024;
/// Maximum relocation/call edges reserved for later implementation auditors.
pub const MAX_CALL_EDGES: usize = 65_536;
/// Maximum runtime object/module bytes reserved for later implementation auditors.
pub const MAX_RUNTIME_ARTIFACT_BYTES: usize = 16 * 1024 * 1024;
/// Maximum retained violations, including the terminal budget violation.
pub const MAX_VIOLATIONS: usize = 256;
/// Universal maximum bytes in one dynamic allocation.
pub const MAX_DYNAMIC_ALLOCATION_BYTES: u64 = 2_147_483_647;
/// Universal maximum owned String length/capacity.
pub const MAX_STRING_BYTES: u64 = 2_147_483_647;
/// Universal maximum Vec element length/capacity.
pub const MAX_VEC_ELEMENTS: u64 = 1_048_576;
/// Universal maximum live allocations in one invocation.
pub const MAX_LIVE_ALLOCATIONS: u64 = 1_048_576;
/// Universal maximum allocation/growth operations in one invocation.
pub const MAX_ALLOCATION_OPERATIONS: u64 = 1_048_576;
/// Universal maximum runtime status transitions in one invocation.
pub const MAX_STATUS_TRANSITIONS: u64 = 4_194_304;

const CHECKED_NATIVE_HEADER: &[u8] = include_bytes!("../include/zryna_ownership_runtime_v1.h");

/// Frozen ownership-runtime ABI version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnershipRuntimeAbiVersion {
    /// First ownership runtime declaration contract.
    V1,
}

/// Exact runtime statuses.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
#[repr(u8)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeStatus {
    /// Success.
    Ok = 0,
    /// Satisfiable request could not be allocated.
    Allocation = 1,
    /// Checked arithmetic or a profile maximum failed.
    Capacity = 2,
    /// A reference-count increment overflowed.
    Refcount = 3,
    /// Input bytes were not UTF-8.
    Utf8 = 4,
    /// Weak upgrade observed no strong owner.
    Expired = 5,
    /// Runtime or host violated the trusted ABI.
    AbiViolation = 255,
}

/// Closed verified disposition of one ownership-runtime status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedStatusDisposition {
    /// Successful operation.
    Success,
    /// Deterministic language-level controlled trap.
    ControlledTrap,
    /// Non-faulting branch outcome.
    Branch,
    /// Trusted ABI or host failure.
    HostFailure,
}

/// Closed verified controlled-trap identity declared by the ownership runtime ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedStatusTrapIdentity {
    /// Allocation failure.
    AllocationV1,
    /// Capacity or checked-arithmetic failure.
    CapacityV1,
    /// Reference-count overflow.
    RefcountV1,
    /// Invalid UTF-8 input.
    Utf8V1,
}

/// Immutable status declaration retained only after exact runtime-ABI verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedStatusDeclaration {
    status: RuntimeStatus,
    disposition: VerifiedStatusDisposition,
    trap_identity: Option<VerifiedStatusTrapIdentity>,
}

impl VerifiedStatusDeclaration {
    /// Returns the exact numeric status identity.
    #[must_use]
    pub const fn status(self) -> RuntimeStatus {
        self.status
    }
    /// Returns the authenticated status disposition.
    #[must_use]
    pub const fn disposition(self) -> VerifiedStatusDisposition {
        self.disposition
    }
    /// Returns the exact authenticated controlled-trap identity, when present.
    #[must_use]
    pub const fn trap_identity(self) -> Option<VerifiedStatusTrapIdentity> {
        self.trap_identity
    }
}

/// Canonical logical operation identities, in ABI order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LogicalOperation {
    /// Raw allocation.
    Allocate,
    /// Raw allocation growth.
    Grow,
    /// Raw allocation release.
    Release,
    /// Copy validated UTF-8 bytes into a String.
    StringFromUtf8Copy,
    /// Deep-clone a String.
    StringClone,
    /// Concatenate two Strings.
    StringConcat,
    /// Release String storage.
    StringRelease,
    /// Allocate fresh Vec storage.
    VecAllocate,
    /// Reserve deterministic Vec storage.
    VecReserve,
    /// Release Vec storage.
    VecReleaseStorage,
    /// Increment a strong count.
    StrongClone,
    /// Create a Weak from Shared.
    WeakDowngrade,
    /// Increment an explicit weak count.
    WeakClone,
    /// Upgrade Weak to Shared.
    WeakUpgrade,
    /// Begin a strong release.
    StrongReleaseBegin,
    /// Finish last-strong payload release.
    StrongReleaseFinish,
    /// Release an explicit Weak.
    WeakRelease,
}

impl LogicalOperation {
    /// Returns the canonical declaration spelling.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Allocate => "allocate",
            Self::Grow => "grow",
            Self::Release => "release",
            Self::StringFromUtf8Copy => "stringFromUtf8Copy",
            Self::StringClone => "stringClone",
            Self::StringConcat => "stringConcat",
            Self::StringRelease => "stringRelease",
            Self::VecAllocate => "vecAllocate",
            Self::VecReserve => "vecReserve",
            Self::VecReleaseStorage => "vecReleaseStorage",
            Self::StrongClone => "strongClone",
            Self::WeakDowngrade => "weakDowngrade",
            Self::WeakClone => "weakClone",
            Self::WeakUpgrade => "weakUpgrade",
            Self::StrongReleaseBegin => "strongReleaseBegin",
            Self::StrongReleaseFinish => "strongReleaseFinish",
            Self::WeakRelease => "weakRelease",
        }
    }
}

const OPERATIONS: [LogicalOperation; 17] = [
    LogicalOperation::Allocate,
    LogicalOperation::Grow,
    LogicalOperation::Release,
    LogicalOperation::StringFromUtf8Copy,
    LogicalOperation::StringClone,
    LogicalOperation::StringConcat,
    LogicalOperation::StringRelease,
    LogicalOperation::VecAllocate,
    LogicalOperation::VecReserve,
    LogicalOperation::VecReleaseStorage,
    LogicalOperation::StrongClone,
    LogicalOperation::WeakDowngrade,
    LogicalOperation::WeakClone,
    LogicalOperation::WeakUpgrade,
    LogicalOperation::StrongReleaseBegin,
    LogicalOperation::StrongReleaseFinish,
    LogicalOperation::WeakRelease,
];

const CONTROL_FORMULAS: [&str; 8] = [
    "endianness=little",
    "prefix-size=8",
    "payload-offset=alignUp(8,payloadAlignment)",
    "control-alignment=max(4,payloadAlignment)",
    "control-size=alignUp(payloadOffset+payloadSize,controlAlignment)",
    "weak-count=explicitWeakHandles+implicitWeakOwnerWhileStrongCountPositive",
    "pending-last-strong=payloadDropBeforeImplicitWeakOwnerRelease",
    "zero-sized-payload-uses-nonzero-allocation=true",
];

const TRANSITION_RULE_IDENTITIES: [&str; 12] = [
    "strong-clone",
    "strong-clone-overflow",
    "weak-downgrade",
    "weak-clone-after-expiry",
    "weak-upgrade",
    "weak-upgrade-expired",
    "strong-release-nonlast",
    "strong-release-last-begin",
    "strong-release-finish-deallocates",
    "strong-release-finish-retains-explicit-weak",
    "weak-release-deallocates",
    "finish-without-pending-last-strong",
];

const TRANSITION_RULE_FINGERPRINTS: [&str; 12] = [
    "strongClone:strong>0&&strong<MAX=>OK,strong+=1",
    "strongClone:strong==MAX=>REFCOUNT,unchanged",
    "weakDowngrade:strong>0&&weak<MAX=>OK,weak+=1",
    "weakClone:allocated&&weak>0&&weak<MAX=>OK,weak+=1",
    "weakUpgrade:strong>0&&strong<MAX=>OK,strong+=1",
    "weakUpgrade:strong==0=>EXPIRED,unchanged",
    "strongReleaseBegin:strong>1=>OK,strong-=1,false",
    "strongReleaseBegin:strong==1=>OK,strong=0,pending=true,true",
    "strongReleaseFinish:pending&&payloadDropped&&weak==1=>OK,deallocate",
    "strongReleaseFinish:pending&&payloadDropped&&weak>1=>OK,weak-=1,retain",
    "weakRelease:explicitWeakPresent=>OK,weak-=1,deallocateIffNoOwners",
    "strongReleaseFinish:!pending=>ABI_VIOLATION,unchanged",
];

const NON_CAPABILITIES: [&str; 25] = [
    "runtime-implementation",
    "allocator-algorithm",
    "allocator-context",
    "public-aggregate-host-abi",
    "rust-standard-library-layout",
    "rust-abi",
    "source-selected-allocator",
    "raw-pointers",
    "threads",
    "atomics",
    "callbacks",
    "finalizers",
    "exceptions",
    "unwinding",
    "tracing-gc",
    "cycle-collection",
    "javascript-weakref",
    "webassembly-gc",
    "wasi",
    "component-model",
    "memory64",
    "shared-memory",
    "windows-native-runtime",
    "macos-native-runtime",
    "freestanding-target",
];

/// Closed untrusted declaration vocabulary.
#[allow(missing_docs)]
pub mod raw {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct Contract {
        pub schema_version: u32,
        pub identifier: String,
        pub layout_claims: Vec<LayoutClaim>,
        pub statuses: Vec<StatusDeclaration>,
        pub operations: Vec<OperationDeclaration>,
        pub javascript: Vec<JavaScriptHelper>,
        pub webassembly: Vec<WebAssemblyFunction>,
        pub native_linux_x86_64: Vec<NativeFunction>,
        pub records: Vec<RecordDeclaration>,
        pub native_header: Vec<u8>,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum LayoutTarget {
        Linear32V1,
        LinuxX8664V1,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct LayoutClaim {
        pub target: LayoutTarget,
        pub universe: [u8; 32],
        pub fingerprint: [u8; 32],
    }
    #[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct StatusDeclaration {
        pub numeric: u8,
        pub name: String,
        pub disposition: StatusDisposition,
        pub trap_identity: Option<String>,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum StatusDisposition {
        Success,
        ControlledTrap,
        Branch,
        HostFailure,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum LogicalParameter {
        ByteSize,
        Alignment,
        Pointer,
        OldByteSize,
        NewByteSize,
        Bytes,
        ByteLength,
        String,
        LeftString,
        RightString,
        ElementLayout,
        RequiredCapacity,
        VecStorage,
        RequiredLength,
        Control,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum LogicalResult {
        Status,
        StatusPointer,
        StatusString,
        StatusVecStorage,
        StatusBool,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct OperationDeclaration {
        pub name: String,
        pub parameters: Vec<LogicalParameter>,
        pub result: LogicalResult,
        pub statuses: Vec<u8>,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum JavaScriptResultShape {
        Status,
        StatusPointer,
        StatusHandle,
        StatusBool,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct JavaScriptHelper {
        pub operation: String,
        pub parameters: Vec<LogicalParameter>,
        pub result: JavaScriptResultShape,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum WebAssemblyLane {
        I32,
        I64,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct WebAssemblyFunction {
        pub operation: String,
        pub parameters: Vec<WebAssemblyLane>,
        pub results: Vec<WebAssemblyLane>,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum NativeCarrier {
        U32,
        U64,
        UintPtr,
        ConstU8Pointer,
        ConstHandlePointer,
        MutHandlePointer,
        MutUintPtrPointer,
        MutU32Pointer,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct NativeFunction {
        pub operation: String,
        pub symbol: String,
        pub parameters: Vec<NativeCarrier>,
        pub result: NativeCarrier,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum RecordTarget {
        Linear32V1,
        LinuxX8664V1,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
    pub enum RecordKind {
        StringHandle,
        VecHandle,
        BoolOutcome,
        ControlBlock { payload_type: u32 },
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum FieldRole {
        Pointer,
        Length,
        Capacity,
        Bool,
        StrongCount,
        WeakCount,
        Payload,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct RecordField {
        pub role: FieldRole,
        pub offset: u64,
        pub size: u64,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct RecordDeclaration {
        pub target: RecordTarget,
        pub kind: RecordKind,
        pub size: u64,
        pub alignment: u64,
        pub fields: Vec<RecordField>,
    }
}

/// Stable violation categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeAbiViolationKind {
    /// Missing, duplicate, unknown, reordered, or mismatched operation/symbol.
    Inventory,
    /// Invalid status, carrier, result, record, handle, or transition shape.
    Contract,
    /// Layout target, identity, fingerprint, or derived record mismatch.
    Layout,
    /// Forbidden ambient capability declaration.
    Capability,
    /// Checked declaration/header structure differs.
    Structure,
    /// Reserved for later fault/cleanup implementation evidence.
    Evidence,
    /// Deterministic verification budget exhausted.
    Budget,
}

/// One deterministic runtime ABI violation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeAbiViolation {
    kind: RuntimeAbiViolationKind,
    declaration_index: Option<usize>,
    message: String,
}

impl RuntimeAbiViolation {
    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self.kind {
            RuntimeAbiViolationKind::Inventory => "ZRYNA-R3001",
            RuntimeAbiViolationKind::Contract => "ZRYNA-R3002",
            RuntimeAbiViolationKind::Layout => "ZRYNA-R3003",
            RuntimeAbiViolationKind::Capability => "ZRYNA-R3004",
            RuntimeAbiViolationKind::Structure => "ZRYNA-R3005",
            RuntimeAbiViolationKind::Evidence => "ZRYNA-R3006",
            RuntimeAbiViolationKind::Budget => "ZRYNA-R3201",
        }
    }
    /// Returns the affected declaration index, when applicable.
    #[must_use]
    pub const fn declaration_index(&self) -> Option<usize> {
        self.declaration_index
    }
    /// Returns the deterministic explanation.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Default)]
struct Violations(Vec<RuntimeAbiViolation>);
impl Violations {
    fn push(
        &mut self,
        kind: RuntimeAbiViolationKind,
        index: Option<usize>,
        message: impl Into<String>,
    ) {
        if self.0.len() < MAX_VIOLATIONS - 1 {
            self.0.push(RuntimeAbiViolation {
                kind,
                declaration_index: index,
                message: message.into(),
            });
        } else if self.0.len() == MAX_VIOLATIONS - 1 {
            self.0.push(RuntimeAbiViolation {
                kind: RuntimeAbiViolationKind::Budget,
                declaration_index: None,
                message: "runtime ABI verification diagnostic budget exhausted".to_owned(),
            });
        }
    }
    fn finish(self) -> Vec<RuntimeAbiViolation> {
        self.0
    }
}

/// Opaque identity of one sealed declaration/layout authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OwnershipRuntimeAbiIdentity([u8; 32]);
impl OwnershipRuntimeAbiIdentity {
    /// Returns deterministic SHA-256 identity bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Opaque operation identity branded by one ABI authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationIdentity {
    owner: OwnershipRuntimeAbiIdentity,
    index: u8,
}

#[derive(Clone, Debug)]
struct ElementLayoutRecord {
    target: StorageTarget,
    element: TypeId,
    stride: u64,
    alignment: u64,
}
#[derive(Clone, Debug)]
struct ControlLayoutRecord {
    target: StorageTarget,
    payload: TypeId,
    payload_offset: u64,
    size: u64,
    alignment: u64,
}

/// Verified ownership-runtime ABI declaration authority.
///
/// ```compile_fail
/// let _ = zryna_ownership_runtime_abi::VerifiedOwnershipRuntimeAbi {};
/// ```
/// ```compile_fail
/// fn raw_recovery(value: &zryna_ownership_runtime_abi::VerifiedOwnershipRuntimeAbi) {
///     let _: &zryna_ownership_runtime_abi::raw::Contract = value.raw();
/// }
/// ```
#[derive(Clone, Debug)]
pub struct VerifiedOwnershipRuntimeAbi {
    identity: OwnershipRuntimeAbiIdentity,
    universe: TypeUniverseIdentity,
    linear32_fingerprint: [u8; 32],
    linux_x86_64_fingerprint: [u8; 32],
    element_layouts: Vec<ElementLayoutRecord>,
    control_layouts: Vec<ControlLayoutRecord>,
    javascript: Vec<raw::JavaScriptHelper>,
    webassembly: Vec<raw::WebAssemblyFunction>,
    native: Vec<raw::NativeFunction>,
    records: Vec<raw::RecordDeclaration>,
}

impl VerifiedOwnershipRuntimeAbi {
    /// Returns the sealed authority identity.
    #[must_use]
    pub const fn identity(&self) -> OwnershipRuntimeAbiIdentity {
        self.identity
    }
    /// Returns the frozen ABI version.
    #[must_use]
    pub const fn version(&self) -> OwnershipRuntimeAbiVersion {
        OwnershipRuntimeAbiVersion::V1
    }
    /// Returns the exact ABI identifier.
    #[must_use]
    pub const fn identifier(&self) -> &'static str {
        OWNERSHIP_RUNTIME_V1_IDENTIFIER
    }
    /// Returns the target-neutral layout universe identity.
    #[must_use]
    pub const fn type_universe_identity(&self) -> TypeUniverseIdentity {
        self.universe
    }
    /// Returns the bound Linear32 layout fingerprint.
    #[must_use]
    pub const fn linear32_fingerprint(&self) -> [u8; 32] {
        self.linear32_fingerprint
    }
    /// Returns the bound Linux x86-64 layout fingerprint.
    #[must_use]
    pub const fn linux_x86_64_fingerprint(&self) -> [u8; 32] {
        self.linux_x86_64_fingerprint
    }
    /// Iterates exact operations in normative order.
    #[must_use]
    pub fn operations(&self) -> impl ExactSizeIterator<Item = VerifiedOperation> {
        OPERATIONS.iter().copied().enumerate().map(|(index, operation)| VerifiedOperation {
            id: OperationIdentity { owner: self.identity, index: u8::try_from(index).unwrap_or(0) },
            operation,
        })
    }
    /// Iterates the exact seven statuses in numeric-contract order.
    #[must_use]
    pub fn statuses(&self) -> impl ExactSizeIterator<Item = RuntimeStatus> {
        [
            RuntimeStatus::Ok,
            RuntimeStatus::Allocation,
            RuntimeStatus::Capacity,
            RuntimeStatus::Refcount,
            RuntimeStatus::Utf8,
            RuntimeStatus::Expired,
            RuntimeStatus::AbiViolation,
        ]
        .into_iter()
    }
    /// Iterates the exact authenticated status declarations in numeric-contract order.
    #[must_use]
    pub fn status_declarations(&self) -> impl ExactSizeIterator<Item = VerifiedStatusDeclaration> {
        use VerifiedStatusDisposition as D;
        use VerifiedStatusTrapIdentity as T;
        [
            VerifiedStatusDeclaration {
                status: RuntimeStatus::Ok,
                disposition: D::Success,
                trap_identity: None,
            },
            VerifiedStatusDeclaration {
                status: RuntimeStatus::Allocation,
                disposition: D::ControlledTrap,
                trap_identity: Some(T::AllocationV1),
            },
            VerifiedStatusDeclaration {
                status: RuntimeStatus::Capacity,
                disposition: D::ControlledTrap,
                trap_identity: Some(T::CapacityV1),
            },
            VerifiedStatusDeclaration {
                status: RuntimeStatus::Refcount,
                disposition: D::ControlledTrap,
                trap_identity: Some(T::RefcountV1),
            },
            VerifiedStatusDeclaration {
                status: RuntimeStatus::Utf8,
                disposition: D::ControlledTrap,
                trap_identity: Some(T::Utf8V1),
            },
            VerifiedStatusDeclaration {
                status: RuntimeStatus::Expired,
                disposition: D::Branch,
                trap_identity: None,
            },
            VerifiedStatusDeclaration {
                status: RuntimeStatus::AbiViolation,
                disposition: D::HostFailure,
                trap_identity: None,
            },
        ]
        .into_iter()
    }
    /// Iterates verified JavaScript logical helper mappings. These have no symbol or storage target.
    #[must_use]
    pub fn javascript_helpers(
        &self,
    ) -> impl ExactSizeIterator<Item = VerifiedJavaScriptHelper<'_>> + '_ {
        self.javascript.iter().enumerate().map(|(index, declaration)| VerifiedJavaScriptHelper {
            operation: self.operation_id(index),
            declaration,
        })
    }
    /// Iterates verified core WebAssembly internal signatures.
    #[must_use]
    pub fn webassembly_functions(
        &self,
    ) -> impl ExactSizeIterator<Item = VerifiedWebAssemblyFunction<'_>> + '_ {
        self.webassembly.iter().enumerate().map(|(index, declaration)| {
            VerifiedWebAssemblyFunction { operation: self.operation_id(index), declaration }
        })
    }
    /// Iterates verified Linux x86-64 native declarations.
    #[must_use]
    pub fn native_linux_x86_64_functions(
        &self,
    ) -> impl ExactSizeIterator<Item = VerifiedNativeFunction<'_>> + '_ {
        self.native.iter().enumerate().map(|(index, declaration)| VerifiedNativeFunction {
            operation: self.operation_id(index),
            declaration,
        })
    }
    /// Iterates verified fixed and payload-derived record declarations.
    #[must_use]
    pub fn records(&self) -> impl ExactSizeIterator<Item = VerifiedRecord<'_>> + '_ {
        self.records.iter().map(|declaration| VerifiedRecord { declaration })
    }
    /// Iterates layout-derived Vec element metadata.
    #[must_use]
    pub fn element_layouts(&self) -> impl ExactSizeIterator<Item = VerifiedElementLayout<'_>> + '_ {
        self.element_layouts.iter().map(|record| VerifiedElementLayout { record })
    }
    /// Iterates layout-derived Shared/Weak control metadata.
    #[must_use]
    pub fn control_layouts(&self) -> impl ExactSizeIterator<Item = VerifiedControlLayout<'_>> + '_ {
        self.control_layouts.iter().map(|record| VerifiedControlLayout { record })
    }
    /// Returns checked repository header evidence. It is not a public C library ABI.
    #[must_use]
    pub const fn native_linux_x86_64_header(&self) -> &'static [u8] {
        CHECKED_NATIVE_HEADER
    }
    fn operation_id(&self, index: usize) -> OperationIdentity {
        OperationIdentity { owner: self.identity, index: u8::try_from(index).unwrap_or(0) }
    }
}

/// Immutable verified operation view.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedOperation {
    id: OperationIdentity,
    operation: LogicalOperation,
}
impl VerifiedOperation {
    /// Returns the branded operation identity.
    #[must_use]
    pub const fn id(self) -> OperationIdentity {
        self.id
    }
    /// Returns the exact logical operation.
    #[must_use]
    pub const fn operation(self) -> LogicalOperation {
        self.operation
    }
}

/// Verified JavaScript logical mapping view.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedJavaScriptHelper<'a> {
    operation: OperationIdentity,
    declaration: &'a raw::JavaScriptHelper,
}
impl<'a> VerifiedJavaScriptHelper<'a> {
    /// Returns the branded logical operation.
    #[must_use]
    pub const fn operation(self) -> OperationIdentity {
        self.operation
    }
    /// Returns logical parameter roles in order.
    #[must_use]
    pub fn parameters(self) -> &'a [raw::LogicalParameter] {
        &self.declaration.parameters
    }
    /// Returns the private result-record shape.
    #[must_use]
    pub const fn result(self) -> raw::JavaScriptResultShape {
        self.declaration.result
    }
}

/// Verified core WebAssembly internal signature view.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedWebAssemblyFunction<'a> {
    operation: OperationIdentity,
    declaration: &'a raw::WebAssemblyFunction,
}
impl<'a> VerifiedWebAssemblyFunction<'a> {
    /// Returns the branded logical operation.
    #[must_use]
    pub const fn operation(self) -> OperationIdentity {
        self.operation
    }
    /// Returns exact parameter lanes.
    #[must_use]
    pub fn parameters(self) -> &'a [raw::WebAssemblyLane] {
        &self.declaration.parameters
    }
    /// Returns exact result lanes.
    #[must_use]
    pub fn results(self) -> &'a [raw::WebAssemblyLane] {
        &self.declaration.results
    }
}

/// Verified Linux x86-64 native declaration view.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedNativeFunction<'a> {
    operation: OperationIdentity,
    declaration: &'a raw::NativeFunction,
}
impl<'a> VerifiedNativeFunction<'a> {
    /// Returns the branded logical operation.
    #[must_use]
    pub const fn operation(self) -> OperationIdentity {
        self.operation
    }
    /// Returns the exact reserved symbol.
    #[must_use]
    pub fn symbol(self) -> &'a str {
        &self.declaration.symbol
    }
    /// Returns exact parameter carriers.
    #[must_use]
    pub fn parameters(self) -> &'a [raw::NativeCarrier] {
        &self.declaration.parameters
    }
    /// Returns the exact result carrier.
    #[must_use]
    pub const fn result(self) -> raw::NativeCarrier {
        self.declaration.result
    }
}

/// Verified target storage/control record view.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedRecord<'a> {
    declaration: &'a raw::RecordDeclaration,
}
impl<'a> VerifiedRecord<'a> {
    /// Returns the storage target.
    #[must_use]
    pub const fn target(self) -> raw::RecordTarget {
        self.declaration.target
    }
    /// Returns the exact record kind.
    #[must_use]
    pub const fn kind(self) -> &'a raw::RecordKind {
        &self.declaration.kind
    }
    /// Returns the byte size.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.declaration.size
    }
    /// Returns the byte alignment.
    #[must_use]
    pub const fn alignment(self) -> u64 {
        self.declaration.alignment
    }
    /// Returns exact record fields.
    #[must_use]
    pub fn fields(self) -> &'a [raw::RecordField] {
        &self.declaration.fields
    }
}

/// Immutable Vec element-layout view.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedElementLayout<'a> {
    record: &'a ElementLayoutRecord,
}
impl VerifiedElementLayout<'_> {
    /// Returns the storage target.
    #[must_use]
    pub const fn target(self) -> StorageTarget {
        self.record.target
    }
    /// Returns the sealed element type.
    #[must_use]
    pub const fn element(self) -> TypeId {
        self.record.element
    }
    /// Returns the checked positive stride.
    #[must_use]
    pub const fn stride(self) -> u64 {
        self.record.stride
    }
    /// Returns the element alignment.
    #[must_use]
    pub const fn alignment(self) -> u64 {
        self.record.alignment
    }

    /// Binds one fresh-allocation transition claim to this exact target and element identity.
    #[must_use]
    pub const fn bind_vec_allocate(
        self,
        requested: u64,
        status: RuntimeStatus,
        pointer: u64,
        length: u64,
        capacity: u64,
    ) -> BoundVecTransitionClaim {
        BoundVecTransitionClaim {
            target: self.target(),
            element: self.element(),
            kind: BoundVecTransitionKind::Allocate { requested, status, pointer, length, capacity },
        }
    }

    /// Binds one reserve transition claim, including its old storage identity, to this exact target
    /// and element identity.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn bind_vec_reserve(
        self,
        old_pointer: u64,
        old_length: u64,
        old_capacity: u64,
        required_length: u64,
        status: RuntimeStatus,
        pointer: u64,
        length: u64,
        capacity: u64,
        input_unchanged: bool,
    ) -> BoundVecTransitionClaim {
        BoundVecTransitionClaim {
            target: self.target(),
            element: self.element(),
            kind: BoundVecTransitionKind::Reserve {
                old_pointer,
                old_length,
                old_capacity,
                required_length,
                status,
                pointer,
                length,
                capacity,
                input_unchanged,
            },
        }
    }
}

/// Opaque Vec transition evidence bound to one exact sealed target and element layout.
///
/// Claims can only be constructed from a [`VerifiedElementLayout`]. The private transition payload
/// additionally records the old storage identity for reserve operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundVecTransitionClaim {
    target: StorageTarget,
    element: TypeId,
    kind: BoundVecTransitionKind,
}

impl BoundVecTransitionClaim {
    /// Returns the sealed storage target that issued this claim.
    #[must_use]
    pub const fn target(self) -> StorageTarget {
        self.target
    }

    /// Returns the sealed element identity that issued this claim.
    #[must_use]
    pub const fn element(self) -> TypeId {
        self.element
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoundVecTransitionKind {
    Allocate {
        requested: u64,
        status: RuntimeStatus,
        pointer: u64,
        length: u64,
        capacity: u64,
    },
    Reserve {
        old_pointer: u64,
        old_length: u64,
        old_capacity: u64,
        required_length: u64,
        status: RuntimeStatus,
        pointer: u64,
        length: u64,
        capacity: u64,
        input_unchanged: bool,
    },
}

/// Immutable Shared/Weak control-layout view.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedControlLayout<'a> {
    record: &'a ControlLayoutRecord,
}

impl VerifiedControlLayout<'_> {
    /// Returns the storage target.
    #[must_use]
    pub const fn target(self) -> StorageTarget {
        self.record.target
    }
    /// Returns the sealed payload type.
    #[must_use]
    pub const fn payload(self) -> TypeId {
        self.record.payload
    }
    /// Returns the payload byte offset after the count prefix.
    #[must_use]
    pub const fn payload_offset(self) -> u64 {
        self.record.payload_offset
    }
    /// Returns the total control allocation size.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.record.size
    }
    /// Returns the control allocation alignment.
    #[must_use]
    pub const fn alignment(self) -> u64 {
        self.record.alignment
    }
}

/// Pure transition claims used to verify normative state/result rules without executing a runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionClaim {
    /// Complete Shared/Weak control-block state transition.
    Control {
        /// Logical operation being validated.
        operation: LogicalOperation,
        /// State before the operation.
        before: ControlState,
        /// Returned status.
        status: RuntimeStatus,
        /// Optional canonical boolean outcome.
        bool_result: Option<bool>,
        /// State after the operation.
        after: ControlState,
    },
    /// Fresh Vec allocation result.
    VecAllocate {
        /// Requested capacity.
        requested: u64,
        /// Returned status.
        status: RuntimeStatus,
        /// Returned pointer token; zero is null.
        pointer: u64,
        /// Returned logical length.
        length: u64,
        /// Returned capacity.
        capacity: u64,
    },
    /// Vec reserve result.
    VecReserve {
        /// Existing length.
        old_length: u64,
        /// Existing capacity.
        old_capacity: u64,
        /// Required resulting length.
        required_length: u64,
        /// Returned status.
        status: RuntimeStatus,
        /// Returned pointer token.
        pointer: u64,
        /// Returned length.
        length: u64,
        /// Returned capacity.
        capacity: u64,
        /// Whether the old input remained byte/state exact on failure.
        input_unchanged: bool,
    },
    /// Count-increment transition.
    CountIncrement {
        /// Count before the call.
        before: u32,
        /// Returned status.
        status: RuntimeStatus,
        /// Count after the call.
        after: u32,
    },
    /// Weak-upgrade observation/increment transition.
    WeakUpgrade {
        /// Strong count before observation.
        before: u32,
        /// Returned status.
        status: RuntimeStatus,
        /// Strong count after the call.
        after: u32,
    },
    /// First half of strong release.
    StrongReleaseBegin {
        /// Strong count before release.
        before: u32,
        /// Returned status.
        status: RuntimeStatus,
        /// Strong count after release.
        after: u32,
        /// Returned canonical last-strong outcome.
        is_last: bool,
    },
    /// Explicit weak-release transition.
    WeakRelease {
        /// Weak count before release.
        before: u32,
        /// Current strong count.
        strong: u32,
        /// Returned status.
        status: RuntimeStatus,
        /// Weak count after release.
        after: u32,
        /// Returned canonical deallocation outcome.
        deallocated: bool,
    },
    /// Generic failed operation result-shape proof.
    FailureAtomic {
        /// Non-success status.
        status: RuntimeStatus,
        /// Whether every output pointer/handle lane is zero.
        outputs_zero: bool,
        /// Whether all input state remained exact.
        input_unchanged: bool,
    },
}

/// Validates one pure transition claim.
///
/// # Errors
///
/// Returns `ZRYNA-R3002` when the claimed result does not implement the frozen transition.
pub fn validate_transition(claim: TransitionClaim) -> Result<(), RuntimeAbiViolation> {
    let valid = match claim {
        TransitionClaim::Control { operation, before, status, bool_result, after } => {
            validate_control_transition(operation, before, status, bool_result, after)
        }
        TransitionClaim::VecAllocate { .. }
        | TransitionClaim::VecReserve { .. }
        | TransitionClaim::FailureAtomic { .. } => false,
        TransitionClaim::CountIncrement { before, status, after } => match status {
            RuntimeStatus::Ok => before > 0 && before < u32::MAX && after == before + 1,
            RuntimeStatus::Refcount => before == u32::MAX && after == before,
            _ => false,
        },
        TransitionClaim::WeakUpgrade { before, status, after } => match status {
            RuntimeStatus::Expired => before == 0 && after == 0,
            RuntimeStatus::Ok => before > 0 && before < u32::MAX && after == before + 1,
            RuntimeStatus::Refcount => before == u32::MAX && after == before,
            _ => false,
        },
        TransitionClaim::StrongReleaseBegin { before, status, after, is_last } => {
            status == RuntimeStatus::Ok
                && before > 0
                && after == before - 1
                && is_last == (before == 1)
        }
        TransitionClaim::WeakRelease { before, strong, status, after, deallocated } => {
            status == RuntimeStatus::Ok
                && before > 0
                && after == before - 1
                && deallocated == (after == 0 && strong == 0)
                && !(after == 0 && strong > 0)
        }
    };
    transition_result(valid)
}

/// Validates one operation-bound atomic failure result.
///
/// The explicit operation prevents a generic failure shape from authenticating a status that the
/// frozen operation declaration does not admit.
///
/// # Errors
///
/// Returns `ZRYNA-R3002` when the status is successful, is not admitted by the operation, or the
/// claimed result does not preserve zero outputs and exact input state.
pub fn validate_failure_atomic_transition(
    operation: LogicalOperation,
    status: RuntimeStatus,
    outputs_zero: bool,
    input_unchanged: bool,
) -> Result<(), RuntimeAbiViolation> {
    transition_result(
        status != RuntimeStatus::Ok
            && operation_accepts_status(operation, status)
            && outputs_zero
            && input_unchanged,
    )
}

/// Validates one Vec transition against an opaque sealed element-layout view.
///
/// The layout supplies the verifier-owned positive stride used for every checked byte-capacity
/// calculation. Only [`TransitionClaim::VecAllocate`] and [`TransitionClaim::VecReserve`] are
/// admitted by this entry point.
///
/// # Errors
///
/// Returns `ZRYNA-R3002` when the claim kind, status, result shape, deterministic capacity, element
/// limit, or checked byte amplification violates the frozen transition.
pub fn validate_vec_transition(
    layout: VerifiedElementLayout<'_>,
    claim: TransitionClaim,
) -> Result<(), RuntimeAbiViolation> {
    let stride = layout.stride();
    let valid = match claim {
        TransitionClaim::VecAllocate { requested, status, pointer, length, capacity } => {
            validate_vec_allocate(requested, status, pointer, length, capacity, stride)
        }
        TransitionClaim::VecReserve {
            old_length,
            old_capacity,
            required_length,
            status,
            pointer,
            length,
            capacity,
            input_unchanged,
        } => validate_vec_reserve(
            old_length,
            old_capacity,
            required_length,
            status,
            pointer,
            length,
            capacity,
            input_unchanged,
            stride,
        ),
        _ => false,
    };
    transition_result(valid)
}

/// Validates target-, element-, and storage-bound Vec transition evidence.
///
/// Unlike [`validate_vec_transition`], this additive entry point rejects replay against a different
/// sealed target or element layout. A successful reserve that requires no growth must retain the
/// exact old storage pointer. Growth and failure retain the frozen checked-capacity, byte
/// amplification, result-shape, and failure-atomic rules.
///
/// # Errors
///
/// Returns `ZRYNA-R3002` when the claim is replayed against another layout or violates the bound
/// Vec transition contract.
pub fn validate_bound_vec_transition(
    layout: VerifiedElementLayout<'_>,
    claim: BoundVecTransitionClaim,
) -> Result<(), RuntimeAbiViolation> {
    let identity_matches = claim.target == layout.target() && claim.element == layout.element();
    let valid = identity_matches
        && match claim.kind {
            BoundVecTransitionKind::Allocate { requested, status, pointer, length, capacity } => {
                validate_vec_allocate(requested, status, pointer, length, capacity, layout.stride())
            }
            BoundVecTransitionKind::Reserve {
                old_pointer,
                old_length,
                old_capacity,
                required_length,
                status,
                pointer,
                length,
                capacity,
                input_unchanged,
            } => {
                validate_vec_reserve(
                    old_length,
                    old_capacity,
                    required_length,
                    status,
                    pointer,
                    length,
                    capacity,
                    input_unchanged,
                    layout.stride(),
                ) && validate_bound_vec_storage_identity(
                    old_pointer,
                    old_length,
                    old_capacity,
                    required_length,
                    status,
                    pointer,
                )
            }
        };
    transition_result(valid)
}

const fn validate_bound_vec_storage_identity(
    old_pointer: u64,
    old_length: u64,
    old_capacity: u64,
    required_length: u64,
    status: RuntimeStatus,
    pointer: u64,
) -> bool {
    let old_storage_well_formed =
        if old_capacity == 0 { old_pointer == 0 && old_length == 0 } else { old_pointer != 0 };
    match status {
        RuntimeStatus::Ok if required_length <= old_capacity => {
            old_storage_well_formed && pointer == old_pointer
        }
        RuntimeStatus::Ok | RuntimeStatus::Allocation | RuntimeStatus::Capacity => {
            old_storage_well_formed
        }
        RuntimeStatus::AbiViolation => true,
        _ => false,
    }
}

fn transition_result(valid: bool) -> Result<(), RuntimeAbiViolation> {
    if valid {
        Ok(())
    } else {
        Err(RuntimeAbiViolation {
            kind: RuntimeAbiViolationKind::Contract,
            declaration_index: None,
            message: "runtime transition claim violates the frozen v1 state/result contract"
                .to_owned(),
        })
    }
}

fn validate_vec_allocate(
    requested: u64,
    status: RuntimeStatus,
    pointer: u64,
    length: u64,
    capacity: u64,
    stride: u64,
) -> bool {
    let request_fits = vec_capacity_fits(requested, stride);
    match status {
        RuntimeStatus::Ok if requested == 0 => pointer == 0 && length == 0 && capacity == 0,
        RuntimeStatus::Ok => request_fits && pointer != 0 && length == 0 && capacity == requested,
        RuntimeStatus::Capacity => !request_fits && pointer == 0 && length == 0 && capacity == 0,
        RuntimeStatus::Allocation => {
            requested > 0 && request_fits && pointer == 0 && length == 0 && capacity == 0
        }
        RuntimeStatus::AbiViolation => pointer == 0 && length == 0 && capacity == 0,
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_vec_reserve(
    old_length: u64,
    old_capacity: u64,
    required_length: u64,
    status: RuntimeStatus,
    pointer: u64,
    length: u64,
    capacity: u64,
    input_unchanged: bool,
    stride: u64,
) -> bool {
    let old_valid = old_length <= old_capacity && vec_capacity_fits(old_capacity, stride);
    let reserved =
        old_valid.then(|| reserve_capacity(old_capacity, required_length, stride)).flatten();
    match status {
        RuntimeStatus::Ok => {
            old_valid
                && reserved.is_some()
                && length == old_length
                && Some(capacity) == reserved
                && (capacity == 0 || pointer != 0)
        }
        RuntimeStatus::Capacity => {
            old_valid
                && reserved.is_none()
                && pointer == 0
                && length == 0
                && capacity == 0
                && input_unchanged
        }
        RuntimeStatus::Allocation => {
            old_valid
                && required_length > old_capacity
                && reserved.is_some()
                && pointer == 0
                && length == 0
                && capacity == 0
                && input_unchanged
        }
        RuntimeStatus::AbiViolation => {
            pointer == 0 && length == 0 && capacity == 0 && input_unchanged
        }
        _ => false,
    }
}

fn vec_capacity_fits(capacity: u64, stride: u64) -> bool {
    stride > 0
        && capacity <= MAX_VEC_ELEMENTS
        && capacity.checked_mul(stride).is_some_and(|bytes| bytes <= MAX_DYNAMIC_ALLOCATION_BYTES)
}

/// Returns whether one status is admitted by an operation's exact v1 status set.
#[must_use]
pub fn operation_accepts_status(operation: LogicalOperation, status: RuntimeStatus) -> bool {
    operation_declaration(operation).statuses.contains(&(status as u8))
}

fn validate_control_transition(
    operation: LogicalOperation,
    before: ControlState,
    status: RuntimeStatus,
    bool_result: Option<bool>,
    after: ControlState,
) -> bool {
    if let Some(valid) = control_transition_preflight(operation, before, status, bool_result, after)
    {
        return valid;
    }
    let unchanged = after == before;
    match operation {
        LogicalOperation::StrongClone => match status {
            RuntimeStatus::Ok => {
                before.strong_count > 0
                    && before.strong_count < u32::MAX
                    && after == ControlState { strong_count: before.strong_count + 1, ..before }
                    && bool_result.is_none()
            }
            RuntimeStatus::Refcount => before.strong_count == u32::MAX && unchanged,
            RuntimeStatus::AbiViolation => unchanged,
            _ => false,
        },
        LogicalOperation::WeakDowngrade => match status {
            RuntimeStatus::Ok => {
                before.strong_count > 0
                    && before.weak_count < u32::MAX
                    && after == ControlState { weak_count: before.weak_count + 1, ..before }
                    && bool_result.is_none()
            }
            RuntimeStatus::Refcount => before.weak_count == u32::MAX && unchanged,
            RuntimeStatus::AbiViolation => unchanged,
            _ => false,
        },
        LogicalOperation::WeakClone => match status {
            RuntimeStatus::Ok => {
                before.weak_count > 0
                    && before.weak_count < u32::MAX
                    && after == ControlState { weak_count: before.weak_count + 1, ..before }
                    && bool_result.is_none()
            }
            RuntimeStatus::Refcount => before.weak_count == u32::MAX && unchanged,
            RuntimeStatus::AbiViolation => unchanged,
            _ => false,
        },
        LogicalOperation::WeakUpgrade => match status {
            RuntimeStatus::Ok => {
                before.strong_count > 0
                    && before.strong_count < u32::MAX
                    && after == ControlState { strong_count: before.strong_count + 1, ..before }
                    && bool_result.is_none()
            }
            RuntimeStatus::Expired => before.strong_count == 0 && unchanged,
            RuntimeStatus::Refcount => before.strong_count == u32::MAX && unchanged,
            RuntimeStatus::AbiViolation => unchanged,
            _ => false,
        },
        LogicalOperation::StrongReleaseBegin => match status {
            RuntimeStatus::Ok if before.strong_count > 1 => {
                bool_result == Some(false)
                    && after == ControlState { strong_count: before.strong_count - 1, ..before }
            }
            RuntimeStatus::Ok if before.strong_count == 1 => {
                bool_result == Some(true)
                    && after
                        == ControlState { strong_count: 0, pending_last_strong: true, ..before }
            }
            RuntimeStatus::AbiViolation => unchanged,
            _ => false,
        },
        LogicalOperation::StrongReleaseFinish => match status {
            RuntimeStatus::Ok => {
                before.pending_last_strong
                    && before.strong_count == 0
                    && !before.payload_initialized
                    && before.weak_count > 0
                    && bool_result.is_none()
                    && after
                        == ControlState {
                            weak_count: before.weak_count - 1,
                            pending_last_strong: false,
                            allocated: before.weak_count > 1,
                            ..before
                        }
            }
            RuntimeStatus::AbiViolation => unchanged,
            _ => false,
        },
        LogicalOperation::WeakRelease => match status {
            RuntimeStatus::Ok => {
                let remaining = before.weak_count.checked_sub(1);
                remaining.is_some_and(|weak_count| {
                    let deallocated = weak_count == 0 && before.strong_count == 0;
                    (!deallocated || !before.pending_last_strong)
                        && bool_result == Some(deallocated)
                        && after == ControlState { weak_count, allocated: !deallocated, ..before }
                })
            }
            RuntimeStatus::AbiViolation => unchanged,
            _ => false,
        },
        _ => false,
    }
}

fn control_transition_preflight(
    operation: LogicalOperation,
    before: ControlState,
    status: RuntimeStatus,
    bool_result: Option<bool>,
    after: ControlState,
) -> Option<bool> {
    if !operation_accepts_status(operation, status)
        || !valid_control_state(before)
        || !valid_control_state(after)
    {
        return Some(false);
    }
    if status != RuntimeStatus::Ok && !control_bool_result_is_zero(operation, bool_result) {
        return Some(false);
    }
    (before.pending_last_strong && operation != LogicalOperation::StrongReleaseFinish).then(|| {
        status == RuntimeStatus::AbiViolation
            && after == before
            && control_bool_result_is_zero(operation, bool_result)
    })
}

const fn control_bool_result_is_zero(
    operation: LogicalOperation,
    bool_result: Option<bool>,
) -> bool {
    match operation {
        LogicalOperation::StrongReleaseBegin | LogicalOperation::WeakRelease => {
            matches!(bool_result, Some(false))
        }
        _ => bool_result.is_none(),
    }
}

const fn valid_control_state(state: ControlState) -> bool {
    if !state.allocated {
        return state.strong_count == 0
            && state.weak_count == 0
            && !state.pending_last_strong
            && !state.payload_initialized;
    }
    if state.pending_last_strong {
        return state.strong_count == 0 && state.weak_count > 0;
    }
    if state.strong_count > 0 {
        state.weak_count > 0 && state.payload_initialized
    } else {
        state.weak_count > 0 && !state.payload_initialized
    }
}

fn reserve_capacity(old: u64, required: u64, stride: u64) -> Option<u64> {
    if !vec_capacity_fits(old, stride) || !vec_capacity_fits(required, stride) {
        return None;
    }
    if required <= old {
        return Some(old);
    }
    let mut candidate = if old == 0 { 4 } else { old.checked_mul(2)? };
    while candidate < required {
        if !vec_capacity_fits(candidate, stride) {
            return Some(required);
        }
        candidate = candidate.checked_mul(2)?;
    }
    Some(if vec_capacity_fits(candidate, stride) { candidate } else { required })
}

/// Builds the exact untrusted v1 declaration claim for the supplied sealed layouts.
///
/// The result remains raw and gains no authority until passed to [`verify_v1`].
#[must_use]
pub fn raw_v1(linear32: &VerifiedLayouts, linux_x86_64: &VerifiedLayouts) -> raw::Contract {
    canonical_contract(linear32, linux_x86_64)
}

/// Verifies exact ownership-runtime ABI declarations and binds them to both sealed layouts.
///
/// # Errors
///
/// Returns deterministic bounded violations and no partial authority.
#[allow(clippy::needless_pass_by_value)]
pub fn verify_v1(
    contract: raw::Contract,
    linear32: &VerifiedLayouts,
    linux_x86_64: &VerifiedLayouts,
) -> Result<VerifiedOwnershipRuntimeAbi, Vec<RuntimeAbiViolation>> {
    let mut errors = Violations::default();
    if !input_within_limits(&contract) {
        errors.push(
            RuntimeAbiViolationKind::Budget,
            None,
            "runtime ABI input exceeds a verification limit",
        );
        return Err(errors.finish());
    }
    if linear32.target() != StorageTarget::Linear32V1
        || linux_x86_64.target() != StorageTarget::LinuxX8664V1
        || linear32.source_map_identity() != linux_x86_64.source_map_identity()
        || linear32.universe_identity() != linux_x86_64.universe_identity()
    {
        errors.push(
            RuntimeAbiViolationKind::Layout,
            None,
            "dual layout authorities are not one exact universe",
        );
        return Err(errors.finish());
    }
    let expected = canonical_contract(linear32, linux_x86_64);
    compare_contract(&contract, &expected, &mut errors);
    if !errors.0.is_empty() {
        return Err(errors.finish());
    }
    let (elements, controls) = derive_metadata(linear32, linux_x86_64).map_err(|message| {
        vec![RuntimeAbiViolation {
            kind: RuntimeAbiViolationKind::Layout,
            declaration_index: None,
            message,
        }]
    })?;
    let identity = fingerprint(linear32, linux_x86_64, &expected, &elements, &controls);
    Ok(VerifiedOwnershipRuntimeAbi {
        identity,
        universe: linear32.universe_identity(),
        linear32_fingerprint: *linear32.fingerprint(),
        linux_x86_64_fingerprint: *linux_x86_64.fingerprint(),
        element_layouts: elements,
        control_layouts: controls,
        javascript: expected.javascript,
        webassembly: expected.webassembly,
        native: expected.native_linux_x86_64,
        records: expected.records,
    })
}

fn input_within_limits(contract: &raw::Contract) -> bool {
    let Some(target_functions) = contract
        .javascript
        .len()
        .checked_add(contract.webassembly.len())
        .and_then(|count| count.checked_add(contract.native_linux_x86_64.len()))
    else {
        return false;
    };
    let mut nested_lengths = contract
        .operations
        .iter()
        .map(|row| row.parameters.len().checked_add(row.statuses.len()))
        .chain(contract.javascript.iter().map(|row| Some(row.parameters.len())))
        .chain(
            contract
                .webassembly
                .iter()
                .map(|row| row.parameters.len().checked_add(row.results.len())),
        )
        .chain(contract.native_linux_x86_64.iter().map(|row| Some(row.parameters.len())))
        .chain(contract.records.iter().map(|row| Some(row.fields.len())));
    let Some(base_nested) = contract.statuses.len().checked_add(contract.layout_claims.len())
    else {
        return false;
    };
    let nested = nested_lengths.try_fold(base_nested, |total, count| total.checked_add(count?));
    contract.operations.len() <= MAX_RUNTIME_OPERATIONS
        && target_functions <= MAX_TARGET_FUNCTIONS
        && contract.records.len() <= MAX_LAYOUT_REFERENCES
        && contract.native_header.len() <= MAX_CHECKED_HEADER_BYTES
        && nested.is_some_and(|count| count <= MAX_DECLARATION_CHILDREN)
}

fn compare_contract(actual: &raw::Contract, expected: &raw::Contract, errors: &mut Violations) {
    if actual.schema_version != expected.schema_version {
        errors.push(
            RuntimeAbiViolationKind::Inventory,
            None,
            "ownership runtime ABI schema version mismatch",
        );
    }
    if actual.identifier != expected.identifier {
        errors.push(
            RuntimeAbiViolationKind::Inventory,
            None,
            "ownership runtime ABI identifier mismatch",
        );
    }
    if actual.layout_claims != expected.layout_claims {
        errors.push(RuntimeAbiViolationKind::Layout, None, "layout authority tuple mismatch");
    }
    if actual.statuses != expected.statuses {
        errors.push(RuntimeAbiViolationKind::Contract, None, "status table mismatch");
    }
    compare_rows(
        &actual.operations,
        &expected.operations,
        RuntimeAbiViolationKind::Inventory,
        "operation",
        errors,
    );
    compare_rows(
        &actual.javascript,
        &expected.javascript,
        RuntimeAbiViolationKind::Contract,
        "JavaScript mapping",
        errors,
    );
    compare_rows(
        &actual.webassembly,
        &expected.webassembly,
        RuntimeAbiViolationKind::Contract,
        "WebAssembly mapping",
        errors,
    );
    compare_rows(
        &actual.native_linux_x86_64,
        &expected.native_linux_x86_64,
        RuntimeAbiViolationKind::Inventory,
        "native mapping",
        errors,
    );
    compare_rows(
        &actual.records,
        &expected.records,
        RuntimeAbiViolationKind::Layout,
        "record",
        errors,
    );
    if actual.native_header != expected.native_header {
        errors.push(
            RuntimeAbiViolationKind::Structure,
            None,
            "checked native header bytes mismatch",
        );
    }
    if !header_matches_declarations(actual) {
        errors.push(
            RuntimeAbiViolationKind::Structure,
            None,
            "checked native header does not encode its native declarations and records",
        );
    }
}

fn compare_rows<T: PartialEq>(
    actual: &[T],
    expected: &[T],
    kind: RuntimeAbiViolationKind,
    label: &str,
    errors: &mut Violations,
) {
    let count = actual.len().max(expected.len());
    for index in 0..count {
        if actual.get(index) != expected.get(index) {
            errors.push(
                kind,
                Some(index),
                format!("{label} declaration differs at ordinal {index}"),
            );
        }
    }
}

fn header_matches_declarations(contract: &raw::Contract) -> bool {
    let header = contract
        .native_header
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    let expected_handle =
        b"typedefstruct{uintptr_tpointer;uint64_tlength;uint64_tcapacity;}zryna_rt_o1_handle;";
    if !contains_bytes(&header, expected_handle) || !native_handle_record_is_exact(contract) {
        return false;
    }
    contract.native_linux_x86_64.iter().all(|function| {
        let Some(operation) =
            OPERATIONS.iter().copied().find(|item| item.name() == function.operation)
        else {
            return false;
        };
        let names = native_parameter_names(operation);
        if names.len() != function.parameters.len() {
            return false;
        }
        let mut prototype = format!("{}{}(", native_c_type(function.result), function.symbol);
        for (index, (carrier, name)) in function.parameters.iter().zip(names).enumerate() {
            if index != 0 {
                prototype.push(',');
            }
            prototype.push_str(native_c_type(*carrier));
            prototype.push_str(name);
        }
        prototype.push_str(");");
        contains_bytes(&header, prototype.as_bytes())
    })
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|window| window == needle)
}

fn native_handle_record_is_exact(contract: &raw::Contract) -> bool {
    let expected_fields = [
        raw::RecordField { role: raw::FieldRole::Pointer, offset: 0, size: 8 },
        raw::RecordField { role: raw::FieldRole::Length, offset: 8, size: 8 },
        raw::RecordField { role: raw::FieldRole::Capacity, offset: 16, size: 8 },
    ];
    [raw::RecordKind::StringHandle, raw::RecordKind::VecHandle].iter().all(|kind| {
        contract.records.iter().any(|record| {
            record.target == raw::RecordTarget::LinuxX8664V1
                && &record.kind == kind
                && record.size == 24
                && record.alignment == 8
                && record.fields == expected_fields
        })
    })
}

const fn native_c_type(carrier: raw::NativeCarrier) -> &'static str {
    match carrier {
        raw::NativeCarrier::U32 => "uint32_t",
        raw::NativeCarrier::U64 => "uint64_t",
        raw::NativeCarrier::UintPtr => "uintptr_t",
        raw::NativeCarrier::ConstU8Pointer => "constuint8_t*",
        raw::NativeCarrier::ConstHandlePointer => "constzryna_rt_o1_handle*",
        raw::NativeCarrier::MutHandlePointer => "zryna_rt_o1_handle*",
        raw::NativeCarrier::MutUintPtrPointer => "uintptr_t*",
        raw::NativeCarrier::MutU32Pointer => "uint32_t*",
    }
}

const fn native_parameter_names(operation: LogicalOperation) -> &'static [&'static str] {
    match operation {
        LogicalOperation::Allocate => &["byte_size", "alignment", "out_pointer"],
        LogicalOperation::Grow => {
            &["pointer", "old_byte_size", "new_byte_size", "alignment", "out_pointer"]
        }
        LogicalOperation::Release => &["pointer", "byte_size", "alignment"],
        LogicalOperation::StringFromUtf8Copy => &["bytes", "byte_length", "out_string"],
        LogicalOperation::StringClone => &["source", "out_string"],
        LogicalOperation::StringConcat => &["left", "right", "out_string"],
        LogicalOperation::StringRelease => &["value"],
        LogicalOperation::VecAllocate => &["element_layout_id", "required_capacity", "out_storage"],
        LogicalOperation::VecReserve => {
            &["element_layout_id", "storage", "required_length", "out_storage"]
        }
        LogicalOperation::VecReleaseStorage => &["element_layout_id", "storage"],
        LogicalOperation::StrongClone
        | LogicalOperation::WeakDowngrade
        | LogicalOperation::WeakClone
        | LogicalOperation::WeakUpgrade
        | LogicalOperation::StrongReleaseFinish => &["control"],
        LogicalOperation::StrongReleaseBegin => &["control", "out_is_last_strong"],
        LogicalOperation::WeakRelease => &["control", "out_deallocated"],
    }
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let mask = alignment.checked_sub(1)?;
    value.checked_add(mask).map(|sum| sum & !mask)
}

fn derive_metadata(
    linear32: &VerifiedLayouts,
    linux: &VerifiedLayouts,
) -> Result<(Vec<ElementLayoutRecord>, Vec<ControlLayoutRecord>), String> {
    let mut elements = Vec::new();
    let mut controls = Vec::new();
    for layouts in [linear32, linux] {
        let mut seen_elements = BTreeSet::new();
        let mut seen_payloads = BTreeSet::new();
        for ty in layouts.types() {
            match ty.category() {
                TypeCategory::Vec => {
                    let element = ty
                        .referenced_type()
                        .ok_or_else(|| "Vec lacks element layout".to_owned())?;
                    if seen_elements.insert(element.index()) {
                        let record = layouts
                            .type_by_id(element)
                            .ok_or_else(|| "Vec element is outside layout universe".to_owned())?;
                        if record.size() == 0 {
                            return Err("Vec element layout is zero-sized".to_owned());
                        }
                        let stride = align_up(record.size(), record.alignment())
                            .ok_or_else(|| "Vec element stride overflows".to_owned())?;
                        elements.push(ElementLayoutRecord {
                            target: layouts.target(),
                            element,
                            stride,
                            alignment: record.alignment(),
                        });
                    }
                }
                TypeCategory::Shared | TypeCategory::Weak => {
                    let payload = ty
                        .referenced_type()
                        .ok_or_else(|| "shared handle lacks payload".to_owned())?;
                    if seen_payloads.insert(payload.index()) {
                        let record = layouts.type_by_id(payload).ok_or_else(|| {
                            "shared payload is outside layout universe".to_owned()
                        })?;
                        let alignment = record.alignment().max(4);
                        let payload_offset = align_up(8, record.alignment())
                            .ok_or_else(|| "control payload offset overflows".to_owned())?;
                        let unaligned_size = payload_offset
                            .checked_add(record.size())
                            .ok_or_else(|| "control size overflows".to_owned())?;
                        let size = align_up(unaligned_size, alignment)
                            .ok_or_else(|| "control size alignment overflows".to_owned())?;
                        controls.push(ControlLayoutRecord {
                            target: layouts.target(),
                            payload,
                            payload_offset,
                            size,
                            alignment,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    Ok((elements, controls))
}

fn fingerprint(
    linear: &VerifiedLayouts,
    linux: &VerifiedLayouts,
    contract: &raw::Contract,
    elements: &[ElementLayoutRecord],
    controls: &[ControlLayoutRecord],
) -> OwnershipRuntimeAbiIdentity {
    let mut hash = Sha256::new();
    hash.update(b"zryna.ownership-runtime-abi.authority-fingerprint.v1\0");
    hash.update(OWNERSHIP_RUNTIME_V1_SCHEMA_VERSION.to_le_bytes());
    hash_string(&mut hash, OWNERSHIP_RUNTIME_V1_IDENTIFIER);
    hash.update(linear.universe_identity().as_bytes());
    hash.update(linear.fingerprint());
    hash.update(linux.fingerprint());
    hash_contract(&mut hash, contract);
    for limit in [
        MAX_RUNTIME_OPERATIONS as u64,
        MAX_TARGET_FUNCTIONS as u64,
        MAX_LAYOUT_REFERENCES as u64,
        MAX_DECLARATION_CHILDREN as u64,
        MAX_CHECKED_HEADER_BYTES as u64,
        MAX_CALL_EDGES as u64,
        MAX_RUNTIME_ARTIFACT_BYTES as u64,
        MAX_DYNAMIC_ALLOCATION_BYTES,
        MAX_STRING_BYTES,
        MAX_VEC_ELEMENTS,
        MAX_LIVE_ALLOCATIONS,
        MAX_ALLOCATION_OPERATIONS,
        MAX_STATUS_TRANSITIONS,
        MAX_VIOLATIONS as u64,
        u64::from(u32::MAX),
        u64::from(u32::MAX),
        1,
        32_768,
        8,
        1,
        2,
        4,
        8,
    ] {
        hash.update(limit.to_le_bytes());
    }
    for formula in CONTROL_FORMULAS {
        hash_string(&mut hash, formula);
    }
    for rule in TRANSITION_RULE_IDENTITIES {
        hash_string(&mut hash, rule);
    }
    for rule in TRANSITION_RULE_FINGERPRINTS {
        hash_string(&mut hash, rule);
    }
    for capability in NON_CAPABILITIES {
        hash_string(&mut hash, capability);
    }
    hash.update(CHECKED_NATIVE_HEADER);
    for record in elements {
        hash.update([target_tag(record.target)]);
        hash.update(record.element.index().to_le_bytes());
        hash.update(record.stride.to_le_bytes());
        hash.update(record.alignment.to_le_bytes());
    }
    for record in controls {
        hash.update([target_tag(record.target)]);
        hash.update(record.payload.index().to_le_bytes());
        hash.update(record.payload_offset.to_le_bytes());
        hash.update(record.size.to_le_bytes());
        hash.update(record.alignment.to_le_bytes());
    }
    OwnershipRuntimeAbiIdentity(hash.finalize().into())
}

fn hash_contract(hash: &mut Sha256, contract: &raw::Contract) {
    hash.update(b"raw-contract\0");
    hash.update(contract.schema_version.to_le_bytes());
    hash_string(hash, &contract.identifier);
    for claim in &contract.layout_claims {
        hash.update([match claim.target {
            raw::LayoutTarget::Linear32V1 => 1,
            raw::LayoutTarget::LinuxX8664V1 => 2,
        }]);
        hash.update(claim.universe);
        hash.update(claim.fingerprint);
    }
    for status in &contract.statuses {
        hash.update([status.numeric]);
        hash_string(hash, &status.name);
        hash.update([status_disposition_tag(status.disposition)]);
        hash_string(hash, status.trap_identity.as_deref().unwrap_or(""));
    }
    for operation in &contract.operations {
        hash.update(operation.name.as_bytes());
        hash.update([0]);
        for parameter in &operation.parameters {
            hash.update([logical_parameter_tag(*parameter)]);
        }
        hash.update([logical_result_tag(operation.result)]);
        hash.update(&operation.statuses);
    }
    for helper in &contract.javascript {
        hash_string(hash, &helper.operation);
        for parameter in &helper.parameters {
            hash.update([logical_parameter_tag(*parameter)]);
        }
        hash.update([javascript_result_tag(helper.result)]);
    }
    for function in &contract.webassembly {
        hash_string(hash, &function.operation);
        for lane in &function.parameters {
            hash.update([wasm_lane_tag(*lane)]);
        }
        hash.update([0xff]);
        for lane in &function.results {
            hash.update([wasm_lane_tag(*lane)]);
        }
    }
    for function in &contract.native_linux_x86_64 {
        hash_string(hash, &function.operation);
        hash_string(hash, &function.symbol);
        for carrier in &function.parameters {
            hash.update([native_carrier_tag(*carrier)]);
        }
        hash.update([native_carrier_tag(function.result)]);
    }
    for record in &contract.records {
        hash.update([record_target_tag(record.target)]);
        match record.kind {
            raw::RecordKind::StringHandle => hash.update([1]),
            raw::RecordKind::VecHandle => hash.update([2]),
            raw::RecordKind::BoolOutcome => hash.update([3]),
            raw::RecordKind::ControlBlock { payload_type } => {
                hash.update([4]);
                hash.update(payload_type.to_le_bytes());
            }
        }
        hash.update(record.size.to_le_bytes());
        hash.update(record.alignment.to_le_bytes());
        for field in &record.fields {
            hash.update([field_role_tag(field.role)]);
            hash.update(field.offset.to_le_bytes());
            hash.update(field.size.to_le_bytes());
        }
    }
    hash.update(contract.native_header.len().to_le_bytes());
    hash.update(&contract.native_header);
}

fn hash_string(hash: &mut Sha256, value: &str) {
    hash.update(value.len().to_le_bytes());
    hash.update(value.as_bytes());
}

const fn status_disposition_tag(value: raw::StatusDisposition) -> u8 {
    match value {
        raw::StatusDisposition::Success => 1,
        raw::StatusDisposition::ControlledTrap => 2,
        raw::StatusDisposition::Branch => 3,
        raw::StatusDisposition::HostFailure => 4,
    }
}

const fn logical_parameter_tag(value: raw::LogicalParameter) -> u8 {
    match value {
        raw::LogicalParameter::ByteSize => 1,
        raw::LogicalParameter::Alignment => 2,
        raw::LogicalParameter::Pointer => 3,
        raw::LogicalParameter::OldByteSize => 4,
        raw::LogicalParameter::NewByteSize => 5,
        raw::LogicalParameter::Bytes => 6,
        raw::LogicalParameter::ByteLength => 7,
        raw::LogicalParameter::String => 8,
        raw::LogicalParameter::LeftString => 9,
        raw::LogicalParameter::RightString => 10,
        raw::LogicalParameter::ElementLayout => 11,
        raw::LogicalParameter::RequiredCapacity => 12,
        raw::LogicalParameter::VecStorage => 13,
        raw::LogicalParameter::RequiredLength => 14,
        raw::LogicalParameter::Control => 15,
    }
}
const fn logical_result_tag(value: raw::LogicalResult) -> u8 {
    match value {
        raw::LogicalResult::Status => 1,
        raw::LogicalResult::StatusPointer => 2,
        raw::LogicalResult::StatusString => 3,
        raw::LogicalResult::StatusVecStorage => 4,
        raw::LogicalResult::StatusBool => 5,
    }
}
const fn javascript_result_tag(value: raw::JavaScriptResultShape) -> u8 {
    match value {
        raw::JavaScriptResultShape::Status => 1,
        raw::JavaScriptResultShape::StatusPointer => 2,
        raw::JavaScriptResultShape::StatusHandle => 3,
        raw::JavaScriptResultShape::StatusBool => 4,
    }
}
const fn wasm_lane_tag(value: raw::WebAssemblyLane) -> u8 {
    match value {
        raw::WebAssemblyLane::I32 => 1,
        raw::WebAssemblyLane::I64 => 2,
    }
}
const fn native_carrier_tag(value: raw::NativeCarrier) -> u8 {
    match value {
        raw::NativeCarrier::U32 => 1,
        raw::NativeCarrier::U64 => 2,
        raw::NativeCarrier::UintPtr => 3,
        raw::NativeCarrier::ConstU8Pointer => 4,
        raw::NativeCarrier::ConstHandlePointer => 5,
        raw::NativeCarrier::MutHandlePointer => 6,
        raw::NativeCarrier::MutUintPtrPointer => 7,
        raw::NativeCarrier::MutU32Pointer => 8,
    }
}
const fn record_target_tag(value: raw::RecordTarget) -> u8 {
    match value {
        raw::RecordTarget::Linear32V1 => 1,
        raw::RecordTarget::LinuxX8664V1 => 2,
    }
}
const fn field_role_tag(value: raw::FieldRole) -> u8 {
    match value {
        raw::FieldRole::Pointer => 1,
        raw::FieldRole::Length => 2,
        raw::FieldRole::Capacity => 3,
        raw::FieldRole::Bool => 4,
        raw::FieldRole::StrongCount => 5,
        raw::FieldRole::WeakCount => 6,
        raw::FieldRole::Payload => 7,
    }
}

const fn target_tag(target: StorageTarget) -> u8 {
    match target {
        StorageTarget::Linear32V1 => 1,
        StorageTarget::LinuxX8664V1 => 2,
    }
}

fn status(
    numeric: u8,
    name: &str,
    disposition: raw::StatusDisposition,
    trap_identity: Option<&str>,
) -> raw::StatusDeclaration {
    raw::StatusDeclaration {
        numeric,
        name: name.to_owned(),
        disposition,
        trap_identity: trap_identity.map(str::to_owned),
    }
}

fn operation_declaration(operation: LogicalOperation) -> raw::OperationDeclaration {
    use raw::{LogicalParameter as P, LogicalResult as R};
    let (parameters, result, statuses): (&[P], R, &[u8]) = match operation {
        LogicalOperation::Allocate => {
            (&[P::ByteSize, P::Alignment], R::StatusPointer, &[0, 1, 2, 255])
        }
        LogicalOperation::Grow => (
            &[P::Pointer, P::OldByteSize, P::NewByteSize, P::Alignment],
            R::StatusPointer,
            &[0, 1, 2, 255],
        ),
        LogicalOperation::Release => {
            (&[P::Pointer, P::ByteSize, P::Alignment], R::Status, &[0, 255])
        }
        LogicalOperation::StringFromUtf8Copy => {
            (&[P::Bytes, P::ByteLength], R::StatusString, &[0, 1, 2, 4, 255])
        }
        LogicalOperation::StringClone => (&[P::String], R::StatusString, &[0, 1, 2, 255]),
        LogicalOperation::StringConcat => {
            (&[P::LeftString, P::RightString], R::StatusString, &[0, 1, 2, 255])
        }
        LogicalOperation::StringRelease => (&[P::String], R::Status, &[0, 255]),
        LogicalOperation::VecAllocate => {
            (&[P::ElementLayout, P::RequiredCapacity], R::StatusVecStorage, &[0, 1, 2, 255])
        }
        LogicalOperation::VecReserve => (
            &[P::ElementLayout, P::VecStorage, P::RequiredLength],
            R::StatusVecStorage,
            &[0, 1, 2, 255],
        ),
        LogicalOperation::VecReleaseStorage => {
            (&[P::ElementLayout, P::VecStorage], R::Status, &[0, 255])
        }
        LogicalOperation::StrongClone
        | LogicalOperation::WeakDowngrade
        | LogicalOperation::WeakClone => (&[P::Control], R::Status, &[0, 3, 255]),
        LogicalOperation::WeakUpgrade => (&[P::Control], R::Status, &[0, 3, 5, 255]),
        LogicalOperation::StrongReleaseBegin | LogicalOperation::WeakRelease => {
            (&[P::Control], R::StatusBool, &[0, 255])
        }
        LogicalOperation::StrongReleaseFinish => (&[P::Control], R::Status, &[0, 255]),
    };
    raw::OperationDeclaration {
        name: operation.name().to_owned(),
        parameters: parameters.to_vec(),
        result,
        statuses: statuses.to_vec(),
    }
}

fn wasm_declaration(operation: LogicalOperation) -> raw::WebAssemblyFunction {
    use raw::WebAssemblyLane::{I32, I64};
    let parameters: &[raw::WebAssemblyLane] = match operation {
        LogicalOperation::Allocate
        | LogicalOperation::StrongReleaseBegin
        | LogicalOperation::WeakRelease => &[I32, I32],
        LogicalOperation::Grow
        | LogicalOperation::StringClone
        | LogicalOperation::VecReleaseStorage => &[I32, I32, I32, I32],
        LogicalOperation::Release
        | LogicalOperation::StringFromUtf8Copy
        | LogicalOperation::StringRelease
        | LogicalOperation::VecAllocate => &[I32, I32, I32],
        LogicalOperation::StringConcat => &[I32, I32, I32, I32, I32, I32, I32],
        LogicalOperation::VecReserve => &[I32, I32, I32, I32, I32, I32],
        _ => &[I32],
    };
    raw::WebAssemblyFunction {
        operation: operation.name().to_owned(),
        parameters: parameters.to_vec(),
        results: vec![
            if matches!(operation, LogicalOperation::Allocate | LogicalOperation::Grow) {
                I64
            } else {
                I32
            },
        ],
    }
}

fn native_declaration(operation: LogicalOperation) -> raw::NativeFunction {
    use raw::NativeCarrier::{
        ConstHandlePointer as CH, ConstU8Pointer as CB, MutHandlePointer as MH,
        MutU32Pointer as M32, MutUintPtrPointer as MP, U32, U64, UintPtr as P,
    };
    let parameters: &[raw::NativeCarrier] = match operation {
        LogicalOperation::Allocate => &[U64, U32, MP],
        LogicalOperation::Grow => &[P, U64, U64, U32, MP],
        LogicalOperation::Release => &[P, U64, U32],
        LogicalOperation::StringFromUtf8Copy => &[CB, U64, MH],
        LogicalOperation::StringClone => &[CH, MH],
        LogicalOperation::StringConcat => &[CH, CH, MH],
        LogicalOperation::StringRelease => &[CH],
        LogicalOperation::VecAllocate => &[U32, U64, MH],
        LogicalOperation::VecReserve => &[U32, CH, U64, MH],
        LogicalOperation::VecReleaseStorage => &[U32, CH],
        LogicalOperation::StrongReleaseBegin | LogicalOperation::WeakRelease => &[P, M32],
        _ => &[P],
    };
    raw::NativeFunction {
        operation: operation.name().to_owned(),
        symbol: format!("zryna_rt_o1_{}", native_suffix(operation)),
        parameters: parameters.to_vec(),
        result: U32,
    }
}

const fn native_suffix(operation: LogicalOperation) -> &'static str {
    match operation {
        LogicalOperation::Allocate => "allocate",
        LogicalOperation::Grow => "grow",
        LogicalOperation::Release => "release",
        LogicalOperation::StringFromUtf8Copy => "string_from_utf8_copy",
        LogicalOperation::StringClone => "string_clone",
        LogicalOperation::StringConcat => "string_concat",
        LogicalOperation::StringRelease => "string_release",
        LogicalOperation::VecAllocate => "vec_allocate",
        LogicalOperation::VecReserve => "vec_reserve",
        LogicalOperation::VecReleaseStorage => "vec_release_storage",
        LogicalOperation::StrongClone => "strong_clone",
        LogicalOperation::WeakDowngrade => "weak_downgrade",
        LogicalOperation::WeakClone => "weak_clone",
        LogicalOperation::WeakUpgrade => "weak_upgrade",
        LogicalOperation::StrongReleaseBegin => "strong_release_begin",
        LogicalOperation::StrongReleaseFinish => "strong_release_finish",
        LogicalOperation::WeakRelease => "weak_release",
    }
}

fn canonical_records(
    linear: &VerifiedLayouts,
    linux: &VerifiedLayouts,
) -> Vec<raw::RecordDeclaration> {
    let mut records = Vec::new();
    for (layouts, target, word) in [
        (linear, raw::RecordTarget::Linear32V1, 4_u64),
        (linux, raw::RecordTarget::LinuxX8664V1, 8_u64),
    ] {
        for kind in [raw::RecordKind::StringHandle, raw::RecordKind::VecHandle] {
            records.push(raw::RecordDeclaration {
                target,
                kind,
                size: word * 3,
                alignment: word,
                fields: vec![
                    raw::RecordField { role: raw::FieldRole::Pointer, offset: 0, size: word },
                    raw::RecordField { role: raw::FieldRole::Length, offset: word, size: word },
                    raw::RecordField {
                        role: raw::FieldRole::Capacity,
                        offset: word * 2,
                        size: word,
                    },
                ],
            });
        }
        records.push(raw::RecordDeclaration {
            target,
            kind: raw::RecordKind::BoolOutcome,
            size: 4,
            alignment: 4,
            fields: vec![raw::RecordField { role: raw::FieldRole::Bool, offset: 0, size: 4 }],
        });
        if let Ok((_, controls)) = derive_metadata_for(layouts) {
            records.extend(controls.into_iter().map(|control| raw::RecordDeclaration {
                target,
                kind: raw::RecordKind::ControlBlock { payload_type: control.payload.index() },
                size: control.size,
                alignment: control.alignment,
                fields: vec![
                    raw::RecordField { role: raw::FieldRole::StrongCount, offset: 0, size: 4 },
                    raw::RecordField { role: raw::FieldRole::WeakCount, offset: 4, size: 4 },
                    raw::RecordField {
                        role: raw::FieldRole::Payload,
                        offset: control.payload_offset,
                        size: layouts
                            .type_by_id(control.payload)
                            .map_or(0, zryna_layout::VerifiedType::size),
                    },
                ],
            }));
        }
    }
    records
}

fn derive_metadata_for(
    layouts: &VerifiedLayouts,
) -> Result<(Vec<ElementLayoutRecord>, Vec<ControlLayoutRecord>), String> {
    let mut elements = Vec::new();
    let mut controls = Vec::new();
    let mut seen_elements = BTreeSet::new();
    let mut seen_payloads = BTreeSet::new();
    for ty in layouts.types() {
        if ty.category() == TypeCategory::Vec {
            let element =
                ty.referenced_type().ok_or_else(|| "Vec lacks element layout".to_owned())?;
            if seen_elements.insert(element.index()) {
                let item =
                    layouts.type_by_id(element).ok_or_else(|| "Vec element missing".to_owned())?;
                if item.size() == 0 {
                    return Err("Vec element layout is zero-sized".to_owned());
                }
                elements.push(ElementLayoutRecord {
                    target: layouts.target(),
                    element,
                    stride: align_up(item.size(), item.alignment())
                        .ok_or_else(|| "stride overflow".to_owned())?,
                    alignment: item.alignment(),
                });
            }
        } else if matches!(ty.category(), TypeCategory::Shared | TypeCategory::Weak) {
            let payload =
                ty.referenced_type().ok_or_else(|| "control payload missing".to_owned())?;
            if seen_payloads.insert(payload.index()) {
                let item = layouts
                    .type_by_id(payload)
                    .ok_or_else(|| "control payload missing".to_owned())?;
                let alignment = item.alignment().max(4);
                let payload_offset =
                    align_up(8, item.alignment()).ok_or_else(|| "offset overflow".to_owned())?;
                let size = align_up(
                    payload_offset
                        .checked_add(item.size())
                        .ok_or_else(|| "size overflow".to_owned())?,
                    alignment,
                )
                .ok_or_else(|| "size overflow".to_owned())?;
                controls.push(ControlLayoutRecord {
                    target: layouts.target(),
                    payload,
                    payload_offset,
                    size,
                    alignment,
                });
            }
        }
    }
    Ok((elements, controls))
}

#[cfg(test)]
mod tests;
