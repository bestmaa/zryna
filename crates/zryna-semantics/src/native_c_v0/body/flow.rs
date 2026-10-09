//! Source-derived conditional foreign steps, separate from executable IR or MIR.

use super::{BoundaryCheck, CallEntry, CleanupEntry, FailureRoute, TrapRequirement, ValueType};
use zryna_syntax::native_c_v0::raw::AbiType;

/// Typed operation whose runtime implementation must independently replay this plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlowStep {
    /// Create zero-initialized caller storage; this is not acquisition of a foreign owner.
    OutputSlot {
        /// Original expression arena ordinal.
        expression: usize,
        /// Fresh caller-storage identity, shared by every local alias.
        token: usize,
        /// Exact output storage category.
        ty: ValueType,
    },
    /// Retain private source/backing storage after length/range or UTF-8 preparation checks.
    PrepareLoan {
        /// Original preparation expression ordinal.
        expression: usize,
        /// Fresh scoped loan identity.
        token: usize,
        /// Original expression supplying retained private storage.
        source_expression: usize,
        /// Original resolved local binding, when the source is a local.
        source_binding: Option<usize>,
        /// Whether backing bytes require UTF-8 encoding rather than vector conversion.
        utf8: bool,
        /// Fixed maximum backing length checked before C entry.
        maximum_bytes: usize,
        /// Required foreign checks and preserved private preparation failure identity.
        traps: Vec<TrapRequirement>,
        /// Conditional reverse foreign prefix if preparation fails.
        cleanup: Vec<CleanupEntry>,
    },
    /// Checked reservation of maximum potential acquisitions, before any C effect.
    Reserve {
        /// Original call expression ordinal.
        expression: usize,
        /// Exact upcoming call identity.
        call: usize,
        /// Maximum simultaneously created non-null obligations declared by this call.
        maximum_new_owners: usize,
        /// Limit shared by live and reserved obligations across the execution instance.
        live_limit: usize,
        /// Required pre-effect reservation failure classification.
        trap: TrapRequirement,
        /// Conditional reverse pre-call foreign prefix on reservation failure.
        cleanup: Vec<CleanupEntry>,
    },
    /// Exact ABI call. Unknown status, conversion and boundary failures need the recorded exit.
    Call {
        /// Original call expression ordinal.
        expression: usize,
        /// Unique call identity within this function.
        call: usize,
        /// Exact operation ordinal in the retained declaration authority.
        operation: usize,
        /// Source primitive's checked versus raw entry contract.
        safety: zryna_syntax::native_c_v0::raw::Safety,
        /// Required C-entry condition; skipped empty release still consumes logically.
        entry: CallEntry,
        /// Exact ordered ABI spellings, preserving C-int versus C-i32.
        carriers: Vec<AbiType>,
        /// Complete caller-output token identities in parameter order.
        outputs: Vec<usize>,
        /// Potential obligations recorded on successful non-null outputs before validation.
        created_owners: Vec<usize>,
        /// Exact declared recoverable status domain, possibly empty.
        recoverable: Vec<u32>,
        /// Mandatory input and returned-scalar checks for the future runtime.
        boundary_checks: Vec<BoundaryCheck>,
        /// Potential creations whose ownership is unresolved after an unknown status.
        unknown_status_unresolved_owners: Vec<usize>,
        /// Unknown status classification before any source status guard.
        unknown_status_route: FailureRoute,
        /// Required classification for a C process failure.
        process_fault_route: FailureRoute,
        /// Conditional reverse pre-call prefix, excluding unconfirmed new acquisitions.
        cleanup: Vec<CleanupEntry>,
    },
    /// Classify this exact status; nonzero is terminal and fallthrough refines only this call.
    StatusGuard {
        /// Original terminal expression in the source guard.
        expression: usize,
        /// Exact dominating status call; integer equality alone is insufficient.
        call: usize,
        /// Declared nonzero statuses that may reach this guarded language exit.
        recoverable: Vec<u32>,
        /// Language outcome only for this exact declared status domain.
        recoverable_route: FailureRoute,
        /// Conditional reverse prefix excluding creations of the refuted call.
        cleanup: Vec<CleanupEntry>,
    },
    /// Read output initialized by the exact dominating status-zero call.
    ReadOutput {
        /// Original read expression ordinal.
        expression: usize,
        /// Exact caller-output storage token.
        slot: usize,
        /// Exact successful call that most recently initialized this storage.
        call: usize,
    },
    /// Transfer an already recorded obligation; validate output before value exposure.
    Take {
        /// Original take expression ordinal.
        expression: usize,
        /// Already recorded obligation transferred to the linear value.
        owner: usize,
        /// Complete paired outputs in declared ABI order.
        slots: Vec<usize>,
        /// Exact successful creating call.
        call: usize,
        /// Captured promise permitting release of malformed metadata.
        malformed_release_allowed: bool,
        /// Obligation retained unresolved when malformed output has no release promise.
        malformed_unresolved_owner: Option<usize>,
        /// Malformed-output failure classification.
        malformed_route: FailureRoute,
        /// Conditional reverse prefix allowed to release on malformed output.
        cleanup: Vec<CleanupEntry>,
    },
    /// Checked private copy; allocation failure retains its later authenticated M3 trap identity.
    Copy {
        /// Original private copy expression ordinal.
        expression: usize,
        /// Validated foreign bytes retained during private preparation.
        owner: usize,
        /// Requirement to preserve the later private preparation's exact failure identity.
        trap: TrapRequirement,
        /// Conditional reverse foreign prefix, including the copied owner.
        cleanup: Vec<CleanupEntry>,
    },
    /// Logical consumption after the existing call returns or safe empty skip; never a second C entry.
    /// Every listed obligation remains unresolved if that release faults, without a retry.
    ConfirmRelease {
        /// Original release expression ordinal.
        expression: usize,
        /// Existing release call, never a second C invocation.
        call: usize,
        /// Exact live obligation consumed only on confirmed return or empty skip.
        owner: usize,
        /// Exact matching operation ordinal in retained declarations.
        operation: usize,
        /// Entire unresolved conditional prefix if this release fails; no retry is allowed.
        unresolved_on_fault: Vec<CleanupEntry>,
        /// Failure classification overriding any prior return, error or trap.
        fault_route: FailureRoute,
    },
    /// End scoped loans and clean still-live conditional obligations before returning the value.
    Return {
        /// Original returned expression ordinal.
        expression: usize,
        /// Exact verified source result category.
        ty: ValueType,
        /// Conditional reverse prefix remaining after explicit releases.
        cleanup: Vec<CleanupEntry>,
    },
}
