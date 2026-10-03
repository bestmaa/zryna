//! Executes each admitted machine instruction and exact terminal classification in source order.

use super::super::{super::invariant_error, cleanup, ledger, state::State};
use cranelift_codegen::ir::{InstBuilder, condcodes::IntCC, types};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::{
    VerifiedEffect,
    contract::{BoundaryCheck, BoundaryExitKind, FailureRoute, FlowStep, TrapRequirement},
    raw::Instruction,
};

pub(super) fn apply(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
) -> Result<(), Diagnostic> {
    for instruction in effect.instructions() {
        match instruction {
            Instruction::ZeroOutput(slot) => {
                let FlowStep::OutputSlot { expression, .. } = effect.operation() else {
                    return Err(invariant_error());
                };
                let ty = if slot.bytes == 4 { types::I32 } else { types::I64 };
                let zero = state.builder.ins().iconst(ty, 0);
                state.builder.ins().stack_store(
                    types::I64,
                    zero,
                    state.frame,
                    i32::try_from(slot.offset).map_err(|_| invariant_error())?,
                );
                let address = state.slot_address(slot.token)?;
                state.set(*expression, address)?;
            }
            Instruction::Reserve { maximum, limit, .. } => {
                if *limit != 64 {
                    return Err(invariant_error());
                }
                let maximum = state.constant(*maximum)?;
                let reserved = state.helper(ledger::RESERVE, &[state.context, maximum])?;
                cleanup::check_or_exit(
                    state,
                    reserved,
                    effect,
                    &BoundaryExitKind::ForeignTrap(TrapRequirement::ForeignResourceLimit),
                    2,
                    None,
                    None,
                )?;
            }
            Instruction::Check(check) => check_boundary(state, effect, check)?,
            Instruction::Invoke { call, operation, arguments, .. } => {
                super::calls::invoke(state, effect, *call, *operation, arguments)?;
            }
            Instruction::ClassifyStatus { call } => super::calls::classify(state, effect, *call)?,
            Instruction::StatusZeroRegister { call, owners } => {
                super::calls::register(state, effect, *call, owners)?;
            }
            Instruction::SettleKnownStatusReservation { call, maximum, .. } => {
                super::calls::settle(state, *call, *maximum)?;
            }
            Instruction::StatusZeroOutputs { call, slots } => output_flags(state, *call, slots)?,
            Instruction::Guard { call } => {
                let (operation, status) = *state.calls.get(call).ok_or_else(invariant_error)?;
                let zero = state.builder.ins().icmp_imm_s(IntCC::Equal, status, 0);
                cleanup::check_or_exit(
                    state,
                    zero,
                    effect,
                    &BoundaryExitKind::ForeignFailure(FailureRoute::DeclaredForeignError),
                    1,
                    Some(operation),
                    Some(status),
                )?;
            }
            Instruction::ReadOutput { call, slot } => read_output(state, effect, *call, *slot)?,
            Instruction::ValidateTake { owner } => {
                let FlowStep::Take { expression, .. } = effect.operation() else {
                    return Err(invariant_error());
                };
                if super::super::storage::take(state, effect, *owner)? {
                    continue;
                }
                let arguments = state.owner_arguments(*owner, 0)?;
                let valid = state.helper(ledger::LOOKUP, &arguments)?;
                checked_metadata(state, valid, effect)?;
                state.set(*expression, arguments[2])?;
            }
            Instruction::ConfirmRelease { owner, .. } => cleanup::confirm_explicit(state, *owner)?,
            Instruction::Return { expression } => {
                let value = state.value(*expression)?;
                cleanup::terminal(
                    state,
                    effect,
                    &BoundaryExitKind::Return,
                    0,
                    None,
                    None,
                    Some(value),
                )?;
                state.terminated = true;
            }
            Instruction::CommitOwners(_) => {
                // Foreign registration already precedes metadata validation; this sealed completion
                // inventory records eligibility for the attached later reverse cleanup edges.
            }
            Instruction::Storage(action) => super::super::storage::stage(state, effect, *action)?,
        }
    }
    Ok(())
}

fn check_boundary(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
    check: &BoundaryCheck,
) -> Result<(), Diagnostic> {
    let value = match check {
        BoundaryCheck::BooleanCarrier { expression } => state.value(*expression)?,
        BoundaryCheck::BooleanResult => {
            let FlowStep::Call { call, .. } = effect.operation() else {
                return Err(invariant_error());
            };
            state.calls.get(call).ok_or_else(invariant_error)?.1
        }
        BoundaryCheck::CountConversion { .. } | BoundaryCheck::Borrow { .. } => {
            return super::super::storage::boundary(state, effect, check);
        }
    };
    let valid = state.builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, value, 1);
    cleanup::check_or_exit(
        state,
        valid,
        effect,
        &BoundaryExitKind::ForeignBoundaryFailure(check.clone()),
        3,
        None,
        None,
    )
}
fn checked_metadata(
    state: &mut State<'_, '_>,
    valid: cranelift_codegen::ir::Value,
    effect: VerifiedEffect<'_>,
) -> Result<(), Diagnostic> {
    // These effects have exactly the host/ABI metadata edge independently admitted by MIR.
    cleanup::check_or_exit(
        state,
        valid,
        effect,
        &BoundaryExitKind::ForeignFailure(FailureRoute::HostAbiFailure),
        3,
        None,
        None,
    )
}

fn read_output(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
    call: usize,
    slot: usize,
) -> Result<(), Diagnostic> {
    let FlowStep::ReadOutput { expression, .. } = effect.operation() else {
        return Err(invariant_error());
    };
    let flag = *state.initialized.get(&slot).ok_or_else(invariant_error)?;
    let initialized = state.builder.ins().stack_load(types::I64, types::I32, flag, 0);
    let expected = state.constant(call.checked_add(1).ok_or_else(invariant_error)?)?;
    let matches = state.builder.ins().icmp(IntCC::Equal, initialized, expected);
    let next = state.builder.create_block();
    let invalid = state.builder.create_block();
    state.builder.ins().brif(matches, next, &[], invalid, &[]);
    state.builder.switch_to_block(invalid);
    // The source seal already proves exact status dominance. A damaged private flag
    // is outside that admitted execution and stops with unresolved obligations.
    state.finish(3, None, None, 0, None)?;
    state.builder.switch_to_block(next);
    let value = state.slot_value(slot)?;
    state.set(*expression, value)?;
    Ok(())
}

fn output_flags(state: &mut State<'_, '_>, call: usize, slots: &[usize]) -> Result<(), Diagnostic> {
    let (_, status) = *state.calls.get(&call).ok_or_else(invariant_error)?;
    let zero = state.builder.ins().iconst(types::I32, 0);
    let initialized = state.constant(call.checked_add(1).ok_or_else(invariant_error)?)?;
    let successful = state.builder.ins().icmp_imm_s(IntCC::Equal, status, 0);
    let flag = state.builder.ins().select(successful, initialized, zero);
    for token in slots {
        let slot = *state.initialized.get(token).ok_or_else(invariant_error)?;
        state.builder.ins().stack_store(types::I64, flag, slot, 0);
    }
    Ok(())
}
