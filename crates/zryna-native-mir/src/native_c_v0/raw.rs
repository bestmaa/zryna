//! Complete untrusted machine claims; copying retained records does not mint an issuer.

use super::abi::{OutputSlot, Signature};
use super::entry::{DispatcherEntry, PrivateEntry};
use zryna_layout::TypeId;
use zryna_native_c_ir::{
    contract::{
        Binding, BoundaryCheck, BoundaryExit, BoundaryOwner, CallEntry, FileId, FlowStep,
        Operation, PrivateOwner, PrivatePreparation, SourceMapIdentity, Span, Statement,
        StorageStage, ValueType,
    },
    raw::{Storage, Value},
};

/// Complete native program claims, including unused declarations.
#[derive(Clone, Debug)]
pub struct Program {
    /// Claimed original source identity.
    pub source_map: SourceMapIdentity,
    /// Exact Linux target, never selected from the host.
    pub target: String,
    /// Claimed dual-layout/runtime descriptors; actual issuers remain retained separately.
    pub storage: Storage,
    /// Independently checked hidden dispatch protocol, separate from public C declarations.
    pub dispatcher: DispatcherEntry,
    /// Every original declaration and its physical C ABI.
    pub operations: Vec<OperationPlan>,
    /// Every original source function.
    pub functions: Vec<Function>,
}
/// Exact original operation plus physical ABI.
#[derive(Clone, Debug)]
pub struct OperationPlan {
    /// Dense declaration ordinal.
    pub index: usize,
    /// Complete original library/symbol/policy record.
    pub declaration: Operation,
    /// Claimed physical signature.
    pub signature: Signature,
}
/// Complete function, source values and machine effects.
#[derive(Clone, Debug)]
pub struct Function {
    /// Original file.
    pub file: FileId,
    /// Original declaration ordinal within file.
    pub ordinal: usize,
    /// Exact declaration span.
    pub span: Span,
    /// Exact original source name.
    pub name: String,
    /// Original typed lexical bindings.
    pub bindings: Vec<Binding>,
    /// Source-ordered private/scalar parameter layouts.
    pub parameters: Vec<TypeId>,
    /// Exact result layout and category.
    pub result: (TypeId, ValueType),
    /// Total scalar C export only, or hidden private entry.
    pub export: Option<usize>,
    /// Compiler-local checked entry; genuine private layouts remain in parameters/result above.
    pub entry: PrivateEntry,
    /// Complete original statement inventory.
    pub statements: Vec<Statement>,
    /// Complete dense source operations and ownership/status provenance.
    pub values: Vec<Value>,
    /// Every private origin with its genuine release requirement.
    pub private_owners: Vec<PrivateOwner>,
    /// Non-overlapping caller output storage.
    pub slots: Vec<OutputSlot>,
    /// Output area padded to sixteen bytes, separately from outgoing call arguments.
    pub output_frame_bytes: u32,
    /// Complete original effect sequence plus explicit machine instruction ordering.
    pub effects: Vec<Effect>,
}
/// Exact source effect, independent physical plan and all terminal exits.
#[derive(Clone, Debug)]
pub struct Effect {
    /// Dense original effect ordinal.
    pub id: usize,
    /// Original typed conditional foreign effect.
    pub operation: FlowStep,
    /// Genuine private storage, stride and fault requirements.
    pub preparation: Option<PrivatePreparation>,
    /// Exact ordered machine actions for this effect.
    pub instructions: Vec<Instruction>,
    /// Complete terminal domains, reverse drops and unresolved/protected ownership.
    pub exits: Vec<BoundaryExit>,
    /// Physical reverse cleanup order for each exact boundary exit, without a retry edge.
    pub exit_instructions: Vec<Vec<ExitInstruction>>,
    /// Conditional successful commitments following the effect.
    pub completed: Vec<BoundaryOwner>,
}
/// Closed terminal cleanup actions, referring only to the attached authenticated boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitInstruction {
    /// End a scoped loan before any owner drop.
    EndLoan(usize),
    /// Attempt one exact reverse-order release from this boundary's drop inventory.
    Release {
        /// Dense reverse drop ordinal within this exact boundary.
        drop: usize,
        /// Exact private or foreign obligation from that drop.
        owner: BoundaryOwner,
    },
    /// A release fault immediately overrides the pending outcome and stops further cleanup.
    StopOnReleaseFailure,
    /// Expose the boundary outcome only after all required releases confirm.
    Finish,
}
/// Source-bound machine argument, never a runtime pointer supplied by a raw caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operand {
    /// Exact source value expression.
    Value(usize),
    /// Pointer component of the same retained foreign byte owner.
    OwnerPointer(usize),
}
/// Closed ordered machine actions; conditional success actions do not assert runtime liveness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Instruction {
    /// Zero a distinct caller slot without making its content readable.
    ZeroOutput(OutputSlot),
    /// Exact private preparation stage under the attached genuine stride/fault plan.
    Storage(StorageStage),
    /// Shared-instance live+reserved check before any C effect.
    Reserve {
        /// Exact upcoming C call identity.
        call: usize,
        /// Maximum conditional non-null acquisitions of that call.
        maximum: usize,
        /// Shared live plus reserved limit, fixed at sixty-four.
        limit: usize,
    },
    /// Exact low-width/count/pointer/loan boundary check.
    Check(BoundaryCheck),
    /// One declaration-bound fixed C call, possibly skipped for canonical empty safe release.
    Invoke {
        /// Exact source call identity.
        call: usize,
        /// Exact retained declaration ordinal.
        operation: usize,
        /// Source operands in physical ABI order.
        arguments: Vec<Operand>,
        /// Always enter or skip the canonical empty safe release.
        entry: CallEntry,
    },
    /// Classify the declared status domain before source guards or output exposure.
    ClassifyStatus {
        /// Exact status-returning call.
        call: usize,
    },
    /// Record successful non-null obligations before output metadata validation.
    StatusZeroRegister {
        /// Exact successful creating call.
        call: usize,
        /// Conditional non-null obligations registered before validation.
        owners: Vec<usize>,
    },
    /// Return unused reservations only on declared success or failure-atomic recoverable status.
    /// Unknown status and process failure retain the unresolved acquisition requirements.
    SettleKnownStatusReservation {
        /// Exact original acquiring call.
        call: usize,
        /// Reserved maximum before entry; successful non-null registrations consume this credit.
        maximum: usize,
        /// Exact declared recoverable codes that create no owner and initialize no output.
        recoverable: Vec<u32>,
    },
    /// Mark only these outputs initialized on the exact successful status-zero edge.
    StatusZeroOutputs {
        /// Exact successful initializing call.
        call: usize,
        /// Caller-output tokens initialized on its status-zero edge only.
        slots: Vec<usize>,
    },
    /// Classify the exact dominating status guard.
    Guard {
        /// Exact dominating call whose recoverable status is guarded.
        call: usize,
    },
    /// Read only the slot initialized by this exact call.
    ReadOutput {
        /// Exact dominating successful call.
        call: usize,
        /// Exact initialized caller-output token.
        slot: usize,
    },
    /// Validate metadata before transferring an already recorded foreign owner.
    ValidateTake {
        /// Already registered owner whose metadata must validate before transfer.
        owner: usize,
    },
    /// Confirm the one matching release; failure preserves unresolved ownership, with no retry.
    ConfirmRelease {
        /// The one prior release call, never a second invocation.
        call: usize,
        /// Exact obligation consumed only on confirmed return or safe empty skip.
        owner: usize,
    },
    /// Prepare a result; transfer follows successful cleanup on the attached Return exit.
    Return {
        /// Original returned source expression; attached exits govern transfer.
        expression: usize,
    },
    /// Append only the effect's conditional successful-completion owners.
    CommitOwners(Vec<BoundaryOwner>),
}
