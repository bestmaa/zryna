//! Exact conditional reverse drops. A failed attempted release stops before any suffix or retry.

use super::{super::invariant_error, ledger, state::State};
use cranelift_codegen::ir::{InstBuilder, condcodes::IntCC, types};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::{
    VerifiedEffect,
    contract::{BoundaryDrop, BoundaryExitKind},
};

pub(super) fn release(
    state: &mut State<'_, '_>,
    owner: usize,
    conditional: bool,
) -> Result<(), Diagnostic> {
    let record = state.record(owner)?;
    let next = state.builder.create_block();
    if conditional {
        let present = state.builder.ins().icmp_imm_s(IntCC::NotEqual, record, 0);
        let active = state.builder.create_block();
        state.builder.ins().brif(present, active, &[], next, &[]);
        state.builder.switch_to_block(active);
    }
    let arguments = state.owner_arguments(owner, 1)?;
    let operation =
        super::admit::release_for(state.environment.function, state.environment.program, owner)
            .ok_or_else(invariant_error)?;
    let reference = *state.environment.releases.get(&operation).ok_or_else(invariant_error)?;
    let call = state.builder.ins().call(reference, &arguments[..5]);
    let confirmed = *state.builder.inst_results(call).first().ok_or_else(invariant_error)?;
    checked_release(state, confirmed)?;
    let null = state.builder.ins().iconst(types::I64, 0);
    let slot = *state.owners.get(&owner).ok_or_else(invariant_error)?;
    state.builder.ins().stack_store(types::I64, null, slot, 0);
    state.builder.ins().jump(next, &[]);
    state.builder.switch_to_block(next);
    Ok(())
}

pub(super) fn confirm_explicit(state: &mut State<'_, '_>, owner: usize) -> Result<(), Diagnostic> {
    let arguments = state.owner_arguments(owner, 2)?;
    let retained = state.helper(ledger::LOOKUP, &arguments)?;
    checked_release(state, retained)?;
    let confirmed = state.helper(ledger::CONFIRM, &[state.context, arguments[1]])?;
    checked_release(state, confirmed)?;
    let null = state.builder.ins().iconst(types::I64, 0);
    let slot = *state.owners.get(&owner).ok_or_else(invariant_error)?;
    state.builder.ins().stack_store(types::I64, null, slot, 0);
    Ok(())
}

fn checked_release(
    state: &mut State<'_, '_>,
    valid: cranelift_codegen::ir::Value,
) -> Result<(), Diagnostic> {
    let next = state.builder.create_block();
    let failed = state.builder.create_block();
    state.builder.ins().brif(valid, next, &[], failed, &[]);
    state.builder.switch_to_block(failed);
    state.finish(3, None, None, 0, None)?;
    state.builder.switch_to_block(next);
    Ok(())
}

pub(super) fn terminal(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
    kind: &BoundaryExitKind,
    tag: u8,
    operation: Option<usize>,
    status: Option<cranelift_codegen::ir::Value>,
    value: Option<cranelift_codegen::ir::Value>,
) -> Result<(), Diagnostic> {
    let exit = effect.exits().iter().find(|exit| &exit.kind == kind).ok_or_else(invariant_error)?;
    if !exit.cleanup_required {
        return Err(invariant_error());
    }
    let mut selected_tag = state.builder.ins().iconst(types::I32, i64::from(tag));
    for drop in &exit.cleanup {
        let BoundaryDrop::Foreign(owner) = drop else {
            return Err(invariant_error());
        };
        if owner.validation_required() {
            let (_, status) =
                *state.calls.get(&owner.creating_call()).ok_or_else(invariant_error)?;
            let successful = state.builder.ins().icmp_imm_s(IntCC::Equal, status, 0);
            let record = state.record(owner.owner_id())?;
            let absent = state.builder.ins().icmp_imm_s(IntCC::Equal, record, 0);
            let malformed = state.builder.ins().band(successful, absent);
            let host_tag = state.builder.ins().iconst(types::I32, 3);
            selected_tag = state.builder.ins().select(malformed, host_tag, selected_tag);
        }
        release(state, owner.owner_id(), true)?;
    }
    state.finish_tag(selected_tag, operation, status, u8::from(tag == 2), value)
}

pub(super) fn check_or_exit(
    state: &mut State<'_, '_>,
    valid: cranelift_codegen::ir::Value,
    effect: VerifiedEffect<'_>,
    kind: &BoundaryExitKind,
    tag: u8,
    operation: Option<usize>,
    status: Option<cranelift_codegen::ir::Value>,
) -> Result<(), Diagnostic> {
    let next = state.builder.create_block();
    let failed = state.builder.create_block();
    state.builder.ins().brif(valid, next, &[], failed, &[]);
    state.builder.switch_to_block(failed);
    terminal(state, effect, kind, tag, operation, status, None)?;
    state.builder.switch_to_block(next);
    Ok(())
}
