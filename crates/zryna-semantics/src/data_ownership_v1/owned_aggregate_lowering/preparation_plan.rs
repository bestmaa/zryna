use std::collections::{BTreeMap, BTreeSet};
use zryna_ir::data_ownership_v1::raw;
use zryna_source::Span;

use super::super::owned_constructor_plan::ConstructorKind;
use super::super::owned_string_read::StringBytes;
use super::super::owner_state::OwnerDelta;
use super::super::type_model::OwnedAggregatePlace;
use super::super::{OwnerState, Ty};
use super::operand_decisions::{ProjectionOperation, ReferenceDecision};
use super::preparation_state::{Checkpoint, PlannedPlace};
use super::projection_topology::ProjectionDescriptor;

#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub(super) struct PreparationFacts {
    pub(super) continued_borrows: BTreeSet<raw::BorrowId>,
    pub(super) structured_values: BTreeMap<u32, (raw::ValueId, Ty)>,
    pub(super) retained_indexed_reads: BTreeSet<raw::PlaceId>,
    pub(super) retained_string_reads: BTreeSet<raw::PlaceId>,
    pub(super) initialized_copy_roots: BTreeSet<raw::PlaceId>,
    pub(super) parameter_borrows: BTreeSet<raw::BorrowId>,
    pub(super) aliases: BTreeMap<String, LexicalAlias>,
    pub(super) next_borrow: u32,
    pub(super) active_borrows: BTreeMap<raw::BorrowId, (raw::PlaceId, raw::BorrowAccess)>,
    pub(super) held_cleanup: [usize; 2],
    pub(super) string_bytes: BTreeMap<raw::PlaceId, u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LexicalAlias {
    pub(super) borrow: raw::BorrowId,
    pub(super) ty: Ty,
    pub(super) access: raw::BorrowAccess,
}

impl PreparationFacts {
    pub(super) fn borrow_active(&self, id: raw::BorrowId) -> bool {
        self.parameter_borrows.contains(&id) || self.active_borrows.contains_key(&id)
    }
    pub(super) fn apply(&mut self, delta: OwnerDelta) {
        super::super::owner_state::apply_owner_delta(&mut self.string_bytes, delta);
    }
}

pub(super) enum Leaf<'f> {
    BorrowRead(raw::BorrowId),
    IndexedCopy {
        source: raw::PlaceId,
        index: raw::ValueId,
        cleanup: raw::CleanupPlanId,
    },
    IndexedClone {
        borrow: raw::BorrowId,
        cleanup: raw::CleanupPlanId,
        prefix: raw::CleanupPlanId,
    },
    HandleAwareIndexedClone {
        borrow: raw::BorrowId,
        cleanup: raw::CleanupPlanId,
        prefix: raw::CleanupPlanId,
    },
    Bool(bool),
    I32(i32),
    String {
        bytes: &'f [u8],
        cleanup: raw::CleanupPlanId,
    },
    Environment {
        key: String,
        cleanup: raw::CleanupPlanId,
    },
    Reference(ReferenceDecision),
    Projection {
        source: OwnedAggregatePlace,
        operation: ProjectionOperation,
    },
    StringClone {
        source: OwnedAggregatePlace,
        bytes: StringBytes,
        cleanup: raw::CleanupPlanId,
    },
    StringConcat {
        left: raw::PlaceId,
        right: raw::PlaceId,
        bytes: StringBytes,
        cleanup: raw::CleanupPlanId,
    },
    AggregateClone {
        source: raw::PlaceId,
        cleanup: raw::CleanupPlanId,
        prefix: raw::CleanupPlanId,
    },
    GenericClone {
        source: raw::PlaceId,
        cleanup: raw::CleanupPlanId,
        prefix: raw::CleanupPlanId,
    },
    HandleAwareClone {
        source: raw::PlaceId,
        cleanup: raw::CleanupPlanId,
        prefix: raw::CleanupPlanId,
    },
    SharedConstruct {
        value: raw::ValueId,
        cleanup: raw::CleanupPlanId,
    },
    SharedClone {
        source: raw::PlaceId,
        cleanup: raw::CleanupPlanId,
    },
    WeakDowngrade {
        source: raw::PlaceId,
        cleanup: raw::CleanupPlanId,
    },
    WeakClone {
        source: raw::PlaceId,
        cleanup: raw::CleanupPlanId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StringOperation {
    Clone,
    Concat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StringRead {
    pub(super) place: raw::PlaceId,
    pub(super) root: raw::PlaceId,
    pub(super) value: Option<raw::ValueId>,
    pub(super) bytes: StringBytes,
}

pub(super) enum Operation<'f> {
    DropTemporary {
        place: raw::PlaceId,
    },
    StructuredValue {
        expression: u32,
        value: raw::ValueId,
    },
    ReplaceProjection {
        place: raw::PlaceId,
        value: raw::ValueId,
    },
    VecPush {
        vector: raw::PlaceId,
        value: raw::ValueId,
        cleanup: raw::CleanupPlanId,
    },
    IndexedEnter {
        end: usize,
        result: usize,
    },
    IndexedExit,
    IndexedEffect(raw::InstructionKind),
    IndexedCopyStorage {
        place: raw::PlaceId,
        value: raw::ValueId,
    },
    GenericClonePrefix {
        id: raw::CleanupPlanId,
        owner: raw::PlaceId,
        actions: usize,
    },
    ScalarEnter {
        kind: super::super::scalar_operations::ScalarOperation,
        end: usize,
        operands: Vec<(raw::ValueId, Ty)>,
    },
    ScalarCommit {
        kind: super::super::scalar_operations::ScalarOperation,
        operands: Vec<(raw::ValueId, Ty)>,
    },
    CallEnter {
        signature: CallSignature,
        end: usize,
        arguments: Vec<raw::CallArgument>,
    },
    CallTransfer {
        value: raw::ValueId,
        owner: raw::PlaceId,
    },
    CallRelease,
    CallCommit {
        signature: CallSignature,
        arguments: Vec<raw::CallArgument>,
        cleanup: raw::CleanupPlanId,
    },
    StringEnter {
        kind: StringOperation,
        end: usize,
        reads: Vec<StringRead>,
    },
    StringRead(StringRead),
    StringExit,
    Enter {
        arity: usize,
        kind: ConstructorKind,
        end: usize,
    },
    Release,
    Prefix {
        id: raw::PlaceId,
        descriptor: ProjectionDescriptor,
    },
    CloneCapacity {
        aggregate: bool,
    },
    Cleanup {
        id: raw::CleanupPlanId,
        actions: usize,
        prefix: Option<raw::PlaceId>,
    },
    Leaf(Leaf<'f>),
    Commit {
        kind: ConstructorKind,
        values: Vec<raw::ValueId>,
    },
    VecCommit {
        values: Vec<raw::ValueId>,
        cleanup: raw::CleanupPlanId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CallKind {
    String,
    Vec,
    Generic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CallSignature {
    pub(super) id: raw::FunctionId,
    pub(super) result: Ty,
    pub(super) parameter: Option<Ty>,
    pub(super) arity: usize,
    pub(super) kind: CallKind,
    pub(super) bytes: Option<StringBytes>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CallParameter {
    Value(Ty),
    Borrow { ty: Ty, access: raw::BorrowAccess },
}

pub(super) fn call_parameters(
    signature: &super::super::function_catalog::FunctionSignature,
) -> Vec<CallParameter> {
    use super::super::function_catalog::FunctionParameterOrder;
    signature
        .parameter_order
        .iter()
        .map(|parameter| match parameter {
            FunctionParameterOrder::Value(index) => {
                CallParameter::Value(signature.parameters[*index as usize])
            }
            FunctionParameterOrder::Borrow(index) => {
                let parameter = signature.borrow_parameters[*index as usize];
                CallParameter::Borrow { ty: parameter.referent, access: parameter.access }
            }
        })
        .collect()
}

pub(super) struct Step<'f> {
    pub(super) operation: Operation<'f>,
    pub(super) ty: Ty,
    pub(super) at: Span,
    pub(super) value: Option<raw::ValueId>,
    pub(super) owners: Vec<OwnerDelta>,
    pub(super) after: Checkpoint,
}

// One flat, affine program. Constructor ranges borrow no independently consumable subplan.
pub(super) struct PreparationPlan<'f> {
    pub(super) start: Checkpoint,
    pub(super) steps: Vec<Step<'f>>,
    pub(super) result: raw::ValueId,
    pub(super) result_type: Ty,
    pub(super) owners: OwnerState,
    pub(super) projections: BTreeMap<(u32, u8, u32), raw::PlaceId>,
    pub(super) moved: BTreeSet<raw::PlaceId>,
    pub(super) partial: BTreeSet<raw::PlaceId>,
    pub(super) places: Vec<PlannedPlace>,
    pub(super) visits: usize,
    pub(super) facts: PreparationFacts,
}
