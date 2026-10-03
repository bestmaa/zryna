//! Successful cleanup must consume the actual outcome, not an unrelated source match.

use super::{error, outcome_shape, raw};
use crate::data_ownership_v1::{
    PlaceIdentity, VerifiedDropAction, VerifiedInstructionKind, VerifiedModule, VerifiedPlace,
    VerifiedProgram, VerifiedTerminatorKind,
};
use zryna_diagnostics::Diagnostic;
use zryna_syntax::command_h1_v1::CommandSyntax;

pub(super) fn verify(
    body: &VerifiedProgram,
    source: &CommandSyntax,
) -> Result<(), Vec<Diagnostic>> {
    if !source.uses_outcome() {
        return Ok(());
    }
    let declaration = u32::try_from(source.syntax().files()[0].data_declarations().len())
        .map_err(|_| vec![error("command outcome declaration exceeds its bound")])?;
    for function in body.modules().flat_map(VerifiedModule::functions) {
        let outcomes = function
            .places()
            .filter(|place| {
                outcome_shape(body.linear32_layouts(), raw::TypeId(place.ty().index()))
                    == Some((0, declaration))
            })
            .map(VerifiedPlace::id)
            .collect::<Vec<_>>();
        if outcomes.is_empty() {
            continue;
        }
        for block in function.blocks() {
            for instruction in block.instructions() {
                if matches!(
                    instruction.kind(),
                    VerifiedInstructionKind::DropPlace
                        | VerifiedInstructionKind::ReplacePlace
                        | VerifiedInstructionKind::GenericReplacePlace
                ) {
                    check(instruction.derived_drop_actions(), &outcomes)?;
                }
            }
            let terminator = block.terminator();
            if terminator.kind() == VerifiedTerminatorKind::Return {
                check(terminator.derived_drop_actions(), &outcomes)?;
            }
        }
    }
    Ok(())
}

fn check(
    actions: impl Iterator<Item = VerifiedDropAction>,
    outcomes: &[PlaceIdentity],
) -> Result<(), Vec<Diagnostic>> {
    for action in actions {
        for place in std::iter::once(action.root()).chain(action.initialized_projections()) {
            if outcomes.contains(&place)
                && !action.active_variants().any(|variant| variant.place() == place)
            {
                return Err(vec![error(
                    "environment outcome must be consumed by an exhaustive match before successful cleanup",
                )]);
            }
        }
    }
    // Existing replay follows exact owner identities through moves and active match arms.
    // Failure/trap cleanup remains available before matching; private calls transfer ownership
    // and their receiving function independently proves its normal cleanup.
    Ok(())
}
