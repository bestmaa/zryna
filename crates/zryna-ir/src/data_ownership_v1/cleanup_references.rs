//! Canonical one-site cleanup-plan bindings.

use super::{VerifiedCleanupRole, raw};

#[derive(Clone, Copy)]
pub(super) struct CleanupReference {
    pub(super) plan: raw::CleanupPlanId,
    pub(super) block: usize,
    pub(super) instruction: Option<usize>,
    pub(super) role: VerifiedCleanupRole,
}

pub(super) fn cleanup_references(function: &raw::Function) -> Vec<CleanupReference> {
    let mut references = Vec::new();
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            if let Some(plan) = instruction_cleanup(&instruction.kind) {
                let role = if matches!(instruction.kind, raw::InstructionKind::DirectCall { .. }) {
                    VerifiedCleanupRole::CallTrap
                } else {
                    VerifiedCleanupRole::PrepareFailure
                };
                references.push(CleanupReference {
                    plan,
                    block: block_index,
                    instruction: Some(instruction_index),
                    role,
                });
            }
            let prefix = match instruction.kind {
                raw::InstructionKind::VecClone { element_cleanup: Some(plan), .. } => {
                    Some((plan, VerifiedCleanupRole::VecCloneElementFailure))
                }
                raw::InstructionKind::ClonePlace { element_cleanup: Some(plan), .. } => {
                    Some((plan, VerifiedCleanupRole::AggregateCloneElementFailure))
                }
                raw::InstructionKind::GenericClonePlace { prefix_cleanup: plan, .. }
                | raw::InstructionKind::GenericCloneBorrow { prefix_cleanup: plan, .. }
                | raw::InstructionKind::HandleAwareClonePlace { prefix_cleanup: plan, .. }
                | raw::InstructionKind::HandleAwareCloneBorrow { prefix_cleanup: plan, .. } => {
                    Some((plan, VerifiedCleanupRole::GenericClonePrefixFailure))
                }
                _ => None,
            };
            if let Some((plan, role)) = prefix {
                references.push(CleanupReference {
                    plan,
                    block: block_index,
                    instruction: Some(instruction_index),
                    role,
                });
            }
        }
        if let Some(terminator) = block.terminators.first() {
            let site = match terminator.kind {
                raw::Terminator::Return { cleanup, .. } => {
                    Some((cleanup, VerifiedCleanupRole::Return))
                }
                raw::Terminator::WeakUpgradeBranch { cleanup, .. } => {
                    Some((cleanup, VerifiedCleanupRole::PrepareFailure))
                }
                raw::Terminator::Trap { cleanup, .. } => {
                    Some((cleanup, VerifiedCleanupRole::ControlledTrap))
                }
                raw::Terminator::Jump(_)
                | raw::Terminator::Branch { .. }
                | raw::Terminator::EnumMatch { .. } => None,
            };
            if let Some((plan, role)) = site {
                references.push(CleanupReference {
                    plan,
                    block: block_index,
                    instruction: None,
                    role,
                });
            }
        }
    }
    references
}

pub(super) fn instruction_cleanup(kind: &raw::InstructionKind) -> Option<raw::CleanupPlanId> {
    use raw::InstructionKind as I;
    match kind {
        I::StructConstruct { cleanup, .. }
        | I::EnumConstruct { cleanup, .. }
        | I::FixedArrayConstruct { cleanup, .. } => *cleanup,
        I::DirectCall { cleanup, .. }
        | I::EnvironmentLookup { cleanup, .. }
        | I::ClonePlace { cleanup, .. }
        | I::GenericClonePlace { cleanup, .. }
        | I::GenericCloneBorrow { cleanup, .. }
        | I::HandleAwareClonePlace { cleanup, .. }
        | I::HandleAwareCloneBorrow { cleanup, .. }
        | I::FixedArrayIndexCopy { cleanup, .. }
        | I::VecIndexCopy { cleanup, .. }
        | I::StringFromUtf8 { cleanup, .. }
        | I::StringClone { cleanup, .. }
        | I::StringConcat { cleanup, .. }
        | I::VecClone { cleanup, .. }
        | I::VecConstruct { cleanup, .. }
        | I::VecPush { cleanup, .. }
        | I::SharedConstruct { cleanup, .. }
        | I::SharedClone { cleanup, .. }
        | I::WeakDowngrade { cleanup, .. }
        | I::WeakClone { cleanup, .. }
        | I::BeginIndexedBorrow { cleanup, .. }
        | I::BeginIndexedAccess { cleanup, .. }
        | I::ProjectIndexedBorrow { cleanup, .. } => Some(*cleanup),
        _ => None,
    }
}
