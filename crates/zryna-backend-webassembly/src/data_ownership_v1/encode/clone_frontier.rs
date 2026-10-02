use super::{Context, failure, index_error};
use wasm_encoder::{BlockType, Function, Instruction as I};
use zryna_ir::data_ownership_v1::{
    VerifiedDropActionKind, VerifiedFunction, VerifiedInstruction, VerifiedInstructionKind as K,
};

// These unexported globals bind one synchronous clone invocation. Type helpers
// recursively call only type helpers; they never re-enter a language callsite.
const MODULE: u32 = 6;
const DECLARATION: u32 = 7;
const ROOT: u32 = 8;
pub(super) const PENDING: u32 = 9;

pub(super) fn required(functions: &[VerifiedFunction<'_>]) -> bool {
    functions
        .iter()
        .flat_map(|function| function.blocks())
        .flat_map(zryna_ir::data_ownership_v1::VerifiedBlock::instructions)
        .any(|instruction| match instruction.kind() {
            K::VecClone => instruction.vec_clone_element_failure_drop_actions().next().is_some(),
            K::ClonePlace => {
                instruction.aggregate_clone_element_failure_drop_actions().next().is_some()
            }
            K::GenericClonePlace | K::GenericCloneBorrow => {
                instruction.generic_clone_prefix_failure_drop_actions().next().is_some()
            }
            K::HandleAwareClonePlace | K::HandleAwareCloneBorrow => {
                instruction.handle_aware_clone_prefix_failure_drop_actions().next().is_some()
            }
            _ => false,
        })
}

pub(super) fn call(
    function: VerifiedFunction<'_>,
    instruction: VerifiedInstruction<'_>,
    index: u32,
    enabled: bool,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    let prefix = match instruction.kind() {
        K::VecClone => instruction.vec_clone_element_failure_drop_actions().next(),
        K::ClonePlace => instruction.aggregate_clone_element_failure_drop_actions().next(),
        K::GenericClonePlace | K::GenericCloneBorrow => {
            instruction.generic_clone_prefix_failure_drop_actions().next()
        }
        K::HandleAwareClonePlace | K::HandleAwareCloneBorrow => {
            instruction.handle_aware_clone_prefix_failure_drop_actions().next()
        }
        _ => None,
    };
    if let Some(prefix) = &prefix {
        if prefix.kind() == VerifiedDropActionKind::Place {
            return Err(index_error());
        }
        if enabled {
            for (global, value) in [
                (MODULE, 0x2000_0000 + function.id().module()),
                (DECLARATION, function.id().declaration()),
                (ROOT, prefix.root().index()),
                (PENDING, 1),
            ] {
                body.instruction(&I::I32Const(i32::try_from(value).map_err(|_| index_error())?));
                body.instruction(&I::GlobalSet(global));
            }
        }
    }
    body.instruction(&I::Call(index));
    if enabled && prefix.is_some() {
        clear(true, body);
    }
    failure::operation_check(body);
    Ok(())
}

pub(super) fn acquired(context: &Context<'_>, body: &mut Function) {
    if !context.clone_frontier {
        return;
    }
    body.instruction(&I::GlobalGet(PENDING));
    body.instruction(&I::I32Const(1));
    body.instruction(&I::I32Eq);
    body.instruction(&I::If(BlockType::Empty));
    body.instruction(&I::I32Const(2));
    body.instruction(&I::GlobalSet(PENDING));
    body.instruction(&I::End);
}

pub(super) fn failed(context: &Context<'_>, body: &mut Function) {
    if !context.clone_frontier {
        return;
    }
    body.instruction(&I::GlobalGet(PENDING));
    body.instruction(&I::I32Const(2));
    body.instruction(&I::I32Eq);
    body.instruction(&I::If(BlockType::Empty));
    for global in [MODULE, DECLARATION, ROOT] {
        body.instruction(&I::GlobalGet(global));
        body.instruction(&I::Call(context.observation + 1));
    }
    body.instruction(&I::End);
    // Preallocation failure consumes the armed state without a destination drop.
    // The first nested failure consumes acquired state before any prefix unwind.
    clear(true, body);
}

pub(super) fn clear(enabled: bool, body: &mut Function) {
    if !enabled {
        return;
    }
    body.instruction(&I::I32Const(0));
    body.instruction(&I::GlobalSet(PENDING));
}
