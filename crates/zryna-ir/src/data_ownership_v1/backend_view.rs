use super::{
    BorrowIdentity, FunctionIdentity, PlaceIdentity, ValueIdentity, VerifiedBorrowAccess,
    VerifiedEdge, VerifiedEnumArm, VerifiedFunction, VerifiedInstruction, VerifiedTerminator,
    VerifiedTrapIdentity, raw, verified_edge,
};

impl VerifiedFunction<'_> {
    /// Returns the sealed referent type for a backend-visible borrow identity.
    #[must_use]
    pub fn backend_borrow_type(self, index: u32) -> Option<zryna_layout::TypeId> {
        self.borrows()
            .definition(raw::BorrowId(index))
            .and_then(|(ty, _)| super::layout_type(&self.owner.linear32, ty))
            .map(zryna_layout::VerifiedType::id)
    }
}

/// Sealed borrow definition consumed by target backends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedBorrowDefinition {
    id: BorrowIdentity,
    place: PlaceIdentity,
    access: VerifiedBorrowAccess,
}

#[allow(missing_docs)]
impl VerifiedBorrowDefinition {
    #[must_use]
    pub const fn id(self) -> BorrowIdentity {
        self.id
    }
    #[must_use]
    pub const fn place(self) -> PlaceIdentity {
        self.place
    }
    #[must_use]
    pub const fn access(self) -> VerifiedBorrowAccess {
        self.access
    }
}

/// Complete immutable instruction view for deterministic target lowering.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum VerifiedBackendInstruction<'a> {
    BoolLiteral(bool),
    I32Literal(i32),
    Binary(ValueIdentity, ValueIdentity),
    Unary(ValueIdentity),
    DirectCall { callee: FunctionIdentity, arguments: Vec<super::VerifiedCallArgument> },
    Construct { operands: Vec<ValueIdentity>, variant: Option<u32> },
    Place(PlaceIdentity),
    PlaceValue { place: PlaceIdentity, value: ValueIdentity },
    IndexedPlace { place: PlaceIdentity, index: ValueIdentity },
    String(&'a [u8]),
    EnvironmentLookup(&'a str),
    StringConcat { left: PlaceIdentity, right: PlaceIdentity },
    VecConstruct(Vec<ValueIdentity>),
    VecPush { vector: PlaceIdentity, value: ValueIdentity },
    BeginBorrow(VerifiedBorrowDefinition),
    IndexedBorrow { definition: VerifiedBorrowDefinition, index: ValueIdentity },
    ProjectIndexedBorrow { parent: BorrowIdentity, borrow: BorrowIdentity, index: ValueIdentity },
    BindIndexedBorrow { parent: BorrowIdentity, borrow: BorrowIdentity },
    BorrowValue { borrow: BorrowIdentity, value: ValueIdentity },
    BorrowUse(BorrowIdentity),
}

impl<'a> VerifiedInstruction<'a> {
    /// Returns all operation-specific operands without exposing raw verifier claims.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn backend_instruction(self) -> VerifiedBackendInstruction<'a> {
        use raw::InstructionKind as I;
        let owner = self.function.id();
        let value = |id: raw::ValueId| ValueIdentity { owner, index: id.0 };
        let place = |id: raw::PlaceId| PlaceIdentity { owner, index: id.0 };
        let borrow = |id: raw::BorrowId| BorrowIdentity { owner, index: id.0 };
        let definition = |raw: &raw::BorrowDefinition| VerifiedBorrowDefinition {
            id: borrow(raw.id),
            place: place(raw.place),
            access: raw.access.into(),
        };
        match &self.instruction.kind {
            I::BoolLiteral(value) => VerifiedBackendInstruction::BoolLiteral(*value),
            I::I32Literal(value) => VerifiedBackendInstruction::I32Literal(*value),
            I::I32Add { lhs, rhs }
            | I::I32Sub { lhs, rhs }
            | I::I32Mul { lhs, rhs }
            | I::Eq { lhs, rhs }
            | I::Ne { lhs, rhs }
            | I::I32LtS { lhs, rhs }
            | I::I32LeS { lhs, rhs }
            | I::I32GtS { lhs, rhs }
            | I::I32GeS { lhs, rhs } => {
                VerifiedBackendInstruction::Binary(value(*lhs), value(*rhs))
            }
            I::I32Neg { operand } => VerifiedBackendInstruction::Unary(value(*operand)),
            I::EnvironmentLookup { key, .. } => VerifiedBackendInstruction::EnvironmentLookup(key),
            I::DirectCall { callee, arguments, .. } => VerifiedBackendInstruction::DirectCall {
                callee: FunctionIdentity {
                    owner: self.function.owner.identity,
                    module: callee.module.0,
                    declaration: callee.declaration,
                },
                arguments: arguments
                    .iter()
                    .map(|argument| match argument {
                        raw::CallArgument::Value(id) => {
                            super::VerifiedCallArgument::Value(value(*id))
                        }
                        raw::CallArgument::Borrow(id) => {
                            super::VerifiedCallArgument::Borrow(borrow(*id))
                        }
                    })
                    .collect(),
            },
            I::StructConstruct { fields, .. } | I::FixedArrayConstruct { elements: fields, .. } => {
                VerifiedBackendInstruction::Construct {
                    operands: fields.iter().copied().map(value).collect(),
                    variant: None,
                }
            }
            I::EnumConstruct { variant, payload, .. } => VerifiedBackendInstruction::Construct {
                operands: payload.iter().copied().map(value).collect(),
                variant: Some(*variant),
            },
            I::CopyFromPlace { place: id }
            | I::MoveFromPlace { place: id }
            | I::GenericMoveFromPlace { place: id }
            | I::ClonePlace { place: id, .. }
            | I::GenericClonePlace { place: id, .. }
            | I::HandleAwareClonePlace { place: id, .. }
            | I::DropPlace { place: id }
            | I::EnumDiscriminant { place: id }
            | I::StringClone { place: id, .. }
            | I::VecClone { place: id, .. }
            | I::SharedClone { place: id, .. }
            | I::WeakDowngrade { place: id, .. }
            | I::WeakClone { place: id, .. } => VerifiedBackendInstruction::Place(place(*id)),
            I::InitializePlace { place: target, value: source }
            | I::ReplacePlace { place: target, value: source }
            | I::GenericReplacePlace { place: target, value: source } => {
                VerifiedBackendInstruction::PlaceValue {
                    place: place(*target),
                    value: value(*source),
                }
            }
            I::FixedArrayIndexCopy { place: base, index, .. }
            | I::VecIndexCopy { place: base, index, .. } => {
                VerifiedBackendInstruction::IndexedPlace {
                    place: place(*base),
                    index: value(*index),
                }
            }
            I::StringFromUtf8 { bytes, .. } => VerifiedBackendInstruction::String(bytes),
            I::StringConcat { left, right, .. } => VerifiedBackendInstruction::StringConcat {
                left: place(*left),
                right: place(*right),
            },
            I::VecConstruct { elements, .. } => VerifiedBackendInstruction::VecConstruct(
                elements.iter().copied().map(value).collect(),
            ),
            I::VecPush { vector, value: source, .. } => VerifiedBackendInstruction::VecPush {
                vector: place(*vector),
                value: value(*source),
            },
            I::SharedConstruct { value: source, .. } => {
                VerifiedBackendInstruction::Unary(value(*source))
            }
            I::BeginBorrow(raw) => VerifiedBackendInstruction::BeginBorrow(definition(raw)),
            I::BeginIndexedBorrow { definition: raw, index, .. }
            | I::BeginIndexedAccess { definition: raw, index, .. } => {
                VerifiedBackendInstruction::IndexedBorrow {
                    definition: definition(raw),
                    index: value(*index),
                }
            }
            I::ProjectIndexedBorrow { parent, borrow: child, index, .. } => {
                VerifiedBackendInstruction::ProjectIndexedBorrow {
                    parent: borrow(*parent),
                    borrow: borrow(*child),
                    index: value(*index),
                }
            }
            I::BindIndexedBorrow { parent, borrow: child } => {
                VerifiedBackendInstruction::BindIndexedBorrow {
                    parent: borrow(*parent),
                    borrow: borrow(*child),
                }
            }
            I::BorrowReplace { borrow: id, value: source }
            | I::BorrowWrite { borrow: id, value: source } => {
                VerifiedBackendInstruction::BorrowValue {
                    borrow: borrow(*id),
                    value: value(*source),
                }
            }
            I::BorrowRead { borrow: id }
            | I::GenericCloneBorrow { borrow: id, .. }
            | I::HandleAwareCloneBorrow { borrow: id, .. }
            | I::EndBorrow { borrow: id } => VerifiedBackendInstruction::BorrowUse(borrow(*id)),
        }
    }
}

/// Complete immutable terminator view for deterministic target lowering.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum VerifiedBackendTerminator {
    Return(ValueIdentity),
    Jump(VerifiedEdge),
    Branch { condition: ValueIdentity, when_true: VerifiedEdge, when_false: VerifiedEdge },
    EnumMatch { place: PlaceIdentity, arms: Vec<VerifiedEnumArm> },
    WeakUpgrade { weak: PlaceIdentity, success: VerifiedEdge, expired: VerifiedEdge },
    Trap(VerifiedTrapIdentity),
}

impl VerifiedTerminator<'_> {
    /// Returns all operation-specific edge and operand data without exposing raw claims.
    #[must_use]
    pub fn backend_terminator(self) -> VerifiedBackendTerminator {
        let owner = self.function.id();
        let value = |id: raw::ValueId| ValueIdentity { owner, index: id.0 };
        let place = |id: raw::PlaceId| PlaceIdentity { owner, index: id.0 };
        match &self.terminator.kind {
            raw::Terminator::Return { value: id, .. } => {
                VerifiedBackendTerminator::Return(value(*id))
            }
            raw::Terminator::Jump(edge) => {
                VerifiedBackendTerminator::Jump(verified_edge(owner, edge))
            }
            raw::Terminator::Branch { condition, when_true, when_false } => {
                VerifiedBackendTerminator::Branch {
                    condition: value(*condition),
                    when_true: verified_edge(owner, when_true),
                    when_false: verified_edge(owner, when_false),
                }
            }
            raw::Terminator::EnumMatch { place: id, arms } => {
                VerifiedBackendTerminator::EnumMatch {
                    place: place(*id),
                    arms: arms
                        .iter()
                        .map(|arm| VerifiedEnumArm {
                            variant: arm.variant,
                            edge: verified_edge(owner, &arm.edge),
                        })
                        .collect(),
                }
            }
            raw::Terminator::WeakUpgradeBranch { weak, success, expired, .. } => {
                VerifiedBackendTerminator::WeakUpgrade {
                    weak: place(*weak),
                    success: verified_edge(owner, success),
                    expired: verified_edge(owner, expired),
                }
            }
            raw::Terminator::Trap { identity, .. } => {
                VerifiedBackendTerminator::Trap(match identity {
                    raw::TrapIdentity::BoundsV1 => VerifiedTrapIdentity::BoundsV1,
                    raw::TrapIdentity::AllocationV1 => VerifiedTrapIdentity::AllocationV1,
                    raw::TrapIdentity::CapacityV1 => VerifiedTrapIdentity::CapacityV1,
                    raw::TrapIdentity::RefcountV1 => VerifiedTrapIdentity::RefcountV1,
                    raw::TrapIdentity::Utf8V1 => VerifiedTrapIdentity::Utf8V1,
                })
            }
        }
    }
}
