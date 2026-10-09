//! Result-place synchronization and derived drop emission.

use super::*;

pub(super) fn sync_result(
    function: VerifiedFunction<'_>,
    instruction: VerifiedInstruction<'_>,
    locals: Locals,
    layouts: &VerifiedLayouts,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    let Some(result) = instruction.result() else { return Ok(()) };
    if let Some(place) =
        function.places().find(|place| place.kind() == VerifiedPlaceKind::Temporary(result))
    {
        place_address(function, place.id().index(), locals, layouts, body)?;
        body.instruction(&Instruction::LocalGet(result.index()));
        store(body);
    }
    Ok(())
}

pub(super) fn emit_instruction_drops(
    function: VerifiedFunction<'_>,
    instruction: VerifiedInstruction<'_>,
    locals: Locals,
    context: &Context<'_>,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    for action in instruction.derived_drop_actions() {
        super::super::cleanup::action(function, &action, locals, context, body)?;
    }
    Ok(())
}

pub(in super::super) fn emit_terminator_drops(
    function: VerifiedFunction<'_>,
    terminator: zryna_ir::data_ownership_v1::VerifiedTerminator<'_>,
    locals: Locals,
    context: &Context<'_>,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    for action in terminator.derived_drop_actions() {
        super::super::cleanup::action(function, &action, locals, context, body)?;
    }
    Ok(())
}
