use super::{Context, Locals};
use wasm_encoder::{BlockType, Function, Instruction};
use zryna_ir::data_ownership_v1::{VerifiedFunction, VerifiedInstruction};

pub(super) fn propagate(
    function: VerifiedFunction<'_>,
    instruction: VerifiedInstruction<'_>,
    locals: Locals,
    context: &Context<'_>,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    body.instruction(&Instruction::GlobalGet(1));
    body.instruction(&Instruction::If(BlockType::Empty));
    for action in if matches!(
        instruction.kind(),
        zryna_ir::data_ownership_v1::VerifiedInstructionKind::StructConstruct
            | zryna_ir::data_ownership_v1::VerifiedInstructionKind::EnumConstruct
            | zryna_ir::data_ownership_v1::VerifiedInstructionKind::FixedArrayConstruct
    ) {
        instruction.allocation_failure_drop_actions().collect::<Vec<_>>()
    } else {
        instruction.derived_drop_actions().collect::<Vec<_>>()
    } {
        super::cleanup::action(function, &action, locals, context, body)?;
    }
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::Return);
    body.instruction(&Instruction::End);
    Ok(())
}

pub(super) fn operation_call(index: u32, body: &mut Function) {
    body.instruction(&Instruction::Call(index));
    operation_check(body);
}

pub(super) fn operation_check(body: &mut Function) {
    body.instruction(&Instruction::GlobalGet(1));
    body.instruction(&Instruction::If(BlockType::Empty));
    body.instruction(&Instruction::Br(1));
    body.instruction(&Instruction::End);
}

pub(super) fn helper_check(context: &Context<'_>, body: &mut Function) {
    body.instruction(&Instruction::GlobalGet(1));
    body.instruction(&Instruction::If(BlockType::Empty));
    super::clone_frontier::failed(context, body);
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::Return);
    body.instruction(&Instruction::End);
}

pub(super) fn helper_trap(code: i32, body: &mut Function) {
    body.instruction(&Instruction::I32Const(code));
    body.instruction(&Instruction::GlobalSet(1));
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::Return);
}
