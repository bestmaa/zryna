//! Aggregate allocation and field initialization for verified instructions.

use super::*;

pub(super) fn construct(
    function: VerifiedFunction<'_>,
    instruction: VerifiedInstruction<'_>,
    locals: Locals,
    context: &Context<'_>,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    let B::Construct { operands, variant } = instruction.backend_instruction() else {
        return Err(index_error());
    };
    let result_type = instruction.result_type().ok_or_else(index_error)?;
    let layout = context.layouts.type_by_id(result_type).ok_or_else(index_error)?;
    body.instruction(&Instruction::I32Const(
        i32::try_from(layout.size().max(1)).map_err(|_| index_error())?,
    ));
    super::super::failure::operation_call(0, body);
    let result = instruction.result().ok_or_else(index_error)?;
    body.instruction(&Instruction::LocalSet(result.index()));
    if layout.category() == TypeCategory::Enum {
        body.instruction(&Instruction::LocalGet(result.index()));
        body.instruction(&Instruction::I32Const(
            i32::try_from(variant.ok_or_else(index_error)?).map_err(|_| index_error())?,
        ));
        store(body);
        if let Some(value) = operands.first() {
            body.instruction(&Instruction::LocalGet(result.index()));
            body.instruction(&Instruction::I32Const(
                i32::try_from(layout.enum_payload_layout().ok_or_else(index_error)?.0)
                    .map_err(|_| index_error())?,
            ));
            body.instruction(&Instruction::I32Add);
            body.instruction(&Instruction::LocalGet(value.index()));
            let variant = layout
                .variants()
                .iter()
                .find(|candidate| candidate.ordinal() == variant.unwrap_or_default())
                .and_then(|candidate| candidate.payload())
                .and_then(|ty| context.layouts.type_by_id(ty))
                .ok_or_else(index_error)?;
            memory::store_value(variant, body);
        }
        return sync_result(function, instruction, locals, context.layouts, body);
    }
    for (index, value) in operands.iter().enumerate() {
        body.instruction(&Instruction::LocalGet(result.index()));
        let offset = if layout.category() == TypeCategory::Struct {
            layout.fields()[index].offset()
        } else {
            layout
                .array_stride()
                .ok_or_else(index_error)?
                .checked_mul(u64::try_from(index).map_err(|_| index_error())?)
                .ok_or_else(index_error)?
        };
        body.instruction(&Instruction::I32Const(i32::try_from(offset).map_err(|_| index_error())?));
        body.instruction(&Instruction::I32Add);
        body.instruction(&Instruction::LocalGet(value.index()));
        let element = if layout.category() == TypeCategory::Struct {
            context.layouts.type_by_id(layout.fields()[index].ty())
        } else {
            layout.referenced_type().and_then(|ty| context.layouts.type_by_id(ty))
        }
        .ok_or_else(index_error)?;
        memory::store_value(element, body);
    }
    return sync_result(function, instruction, locals, context.layouts, body);
}
