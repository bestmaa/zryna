//! Ordered independent admission; no producer helper is called or used as an oracle.

use super::{MirError, VerifiedMirProgram, raw, require};
use zryna_ir::data_ownership_v1 as limits;
use zryna_native_c_ir::VerifiedNativeCProgram;

mod abi;
mod entry;
mod exits;
mod source;
mod storage;

/// Independently verifies complete hostile claims against the genuine retained extension IR.
/// # Errors
/// Rejects budgets, identity, source, physical ABI, storage/order and terminal edges atomically.
pub fn verify(
    program: raw::Program,
    source: &VerifiedNativeCProgram,
) -> Result<VerifiedMirProgram, MirError> {
    preflight(&program)?;
    source::check(&program, source)?;
    entry::check(&program, source)?;
    machine_budget(&program)?;
    abi::check(&program, source)?;
    for (function, original) in program.functions.iter().zip(source.functions()) {
        storage::check(function, original, &program.operations).map_err(|e| e.at(function.span))?;
        exits::check(function, original).map_err(|e| e.at(function.span))?;
    }
    Ok(VerifiedMirProgram { program, source: source.clone() })
}

fn machine_budget(program: &raw::Program) -> Result<(), MirError> {
    let mut actions = 0_usize;
    let mut source_bound = 0_usize;
    for function in &program.functions {
        for effect in &function.effects {
            source_bound = count(source_bound, 32, usize::MAX)?;
            actions = count(actions, effect.instructions.len(), usize::MAX)?;
            require(
                effect.exit_instructions.len() == effect.exits.len(),
                "ZRYNA-C4106",
                "mir-complete-terminal-inventory",
            )?;
            // The exact source payload has already been checked. Only vector lengths are read
            // here, before any raw terminal instruction is traversed or used for allocation.
            for (actions_claim, exit) in effect.exit_instructions.iter().zip(&effect.exits) {
                let bound = exit
                    .cleanup
                    .len()
                    .checked_mul(2)
                    .and_then(|drops| drops.checked_add(exit.end_loans.len()))
                    .and_then(|prefix| prefix.checked_add(1))
                    .ok_or_else(|| MirError::new("ZRYNA-C4107", "mir-terminal-expansion"))?;
                require(actions_claim.len() <= bound, "ZRYNA-C4107", "mir-terminal-amplification")?;
                source_bound = count(source_bound, bound, usize::MAX)?;
                actions = count(actions, actions_claim.len(), usize::MAX)?;
            }
        }
    }
    require(actions <= source_bound, "ZRYNA-C4107", "mir-source-action-budget")
}

fn count(current: usize, added: usize, maximum: usize) -> Result<usize, MirError> {
    current
        .checked_add(added)
        .filter(|value| *value <= maximum)
        .ok_or_else(|| MirError::new("ZRYNA-C4107", "mir-inventory-budget"))
}
fn preflight(program: &raw::Program) -> Result<(), MirError> {
    count(0, program.functions.len(), 4096)?;
    count(0, program.operations.len(), 256)?;
    count(0, program.target.len(), 128)?;
    count(0, program.storage.runtime.len(), 128)?;
    count(0, program.dispatcher.symbol.len(), 128)?;
    count(0, program.dispatcher.contract.len(), 128)?;
    let mut values = 0;
    let mut parameters = 0;
    let mut statements = 0;
    let mut slots = 0;
    for operation in &program.operations {
        count(0, operation.signature.parameters.len(), 16)?;
    }
    for function in &program.functions {
        count(0, function.name.len(), 128)?;
        count(0, function.entry.symbol.len(), 128)?;
        count(0, function.entry.contract.len(), 128)?;
        count(0, function.values.len(), limits::MAX_VALUES_PER_FUNCTION)?;
        values = count(values, function.values.len(), limits::MAX_VALUES_PER_PROGRAM)?;
        count(0, function.parameters.len(), limits::MAX_PARAMETERS_PER_FUNCTION)?;
        count(0, function.bindings.len(), limits::MAX_PARAMETERS_PER_FUNCTION)?;
        parameters =
            count(parameters, function.parameters.len(), limits::MAX_PARAMETERS_PER_PROGRAM)?;
        count(0, function.statements.len(), 4096)?;
        statements = count(statements, function.statements.len(), 65_536)?;
        count(0, function.effects.len(), limits::MAX_OWNERSHIP_TRANSITIONS_PER_FUNCTION)?;
        count(0, function.slots.len(), limits::MAX_VALUES_PER_FUNCTION)?;
        slots = count(slots, function.slots.len(), limits::MAX_VALUES_PER_PROGRAM)?;
        count(0, function.private_owners.len(), limits::MAX_PLACES_PER_FUNCTION)?;
        for effect in &function.effects {
            count(0, effect.instructions.len(), 32)?;
        }
    }
    // Source admission next checks every original payload and all nested vector lengths against
    // bounded sealed records before machine verification traverses or allocates from any claim.
    require(
        values <= limits::MAX_VALUES_PER_PROGRAM && slots <= values,
        "ZRYNA-C4107",
        "mir-output-amplification",
    )
}
