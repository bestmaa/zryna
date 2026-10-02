//! Value-use and consuming-value traversal shared by ownership flow verification.

use super::raw;

pub(super) fn consuming_instruction_operands(kind: &raw::InstructionKind) -> Vec<raw::ValueId> {
    use raw::InstructionKind as I;
    match kind {
        I::DirectCall { arguments, .. } => arguments
            .iter()
            .filter_map(|argument| match argument {
                raw::CallArgument::Value(value) => Some(*value),
                raw::CallArgument::Borrow(_) => None,
            })
            .collect(),
        I::StructConstruct { fields, .. } => fields.clone(),
        I::EnumConstruct { payload, .. } => payload.iter().copied().collect(),
        I::FixedArrayConstruct { elements, .. } | I::VecConstruct { elements, .. } => {
            elements.clone()
        }
        I::InitializePlace { value, .. }
        | I::ReplacePlace { value, .. }
        | I::GenericReplacePlace { value, .. }
        | I::VecPush { value, .. }
        | I::SharedConstruct { value, .. }
        | I::BorrowWrite { value, .. }
        | I::BorrowReplace { value, .. } => vec![*value],
        _ => vec![],
    }
}

pub(super) fn instruction_operands(kind: &raw::InstructionKind) -> Vec<raw::ValueId> {
    use raw::InstructionKind as I;
    match kind {
        I::I32Add { lhs, rhs }
        | I::I32Sub { lhs, rhs }
        | I::I32Mul { lhs, rhs }
        | I::Eq { lhs, rhs }
        | I::Ne { lhs, rhs }
        | I::I32LtS { lhs, rhs }
        | I::I32LeS { lhs, rhs }
        | I::I32GtS { lhs, rhs }
        | I::I32GeS { lhs, rhs } => vec![*lhs, *rhs],
        I::I32Neg { operand } => vec![*operand],
        I::DirectCall { arguments, .. } => arguments
            .iter()
            .filter_map(|argument| match argument {
                raw::CallArgument::Value(value) => Some(*value),
                raw::CallArgument::Borrow(_) => None,
            })
            .collect(),
        I::StructConstruct { fields, .. } => fields.clone(),
        I::EnumConstruct { payload, .. } => payload.iter().copied().collect(),
        I::FixedArrayConstruct { elements, .. } | I::VecConstruct { elements, .. } => {
            elements.clone()
        }
        I::InitializePlace { value, .. }
        | I::ReplacePlace { value, .. }
        | I::GenericReplacePlace { value, .. }
        | I::VecPush { value, .. }
        | I::SharedConstruct { value, .. }
        | I::BorrowWrite { value, .. }
        | I::BorrowReplace { value, .. } => vec![*value],
        I::FixedArrayIndexCopy { index, .. }
        | I::VecIndexCopy { index, .. }
        | I::BeginIndexedBorrow { index, .. }
        | I::BeginIndexedAccess { index, .. }
        | I::ProjectIndexedBorrow { index, .. } => vec![*index],
        _ => vec![],
    }
}
