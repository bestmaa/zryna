//! Foreign call/status ordering, reserved acquisitions and output validity before exposure.

use super::super::{super::invariant_error, cleanup, ledger, state::State};
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, condcodes::IntCC, types};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::{
    VerifiedEffect,
    contract::{AbiType, Access, BoundaryExitKind, FailureRoute, FlowStep},
    raw::Operand,
};

pub(super) fn invoke(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
    call: usize,
    operation: usize,
    operands: &[Operand],
) -> Result<(), Diagnostic> {
    let mut arguments = Vec::with_capacity(operands.len());
    for operand in operands {
        let Operand::Value(expression) = operand else {
            return Err(invariant_error());
        };
        arguments.push(state.value(*expression)?);
    }
    let declaration = state
        .environment
        .program
        .operations()
        .nth(operation)
        .ok_or_else(invariant_error)?
        .declaration();
    for (index, parameter) in declaration.parameters.iter().enumerate() {
        if parameter.abi != AbiType::HandleIn {
            continue;
        }
        let Operand::Value(expression) = operands[index] else {
            return Err(invariant_error());
        };
        let owner = state.handle_owner(expression)?;
        let resource = parameter
            .resource
            .and_then(|group| declaration.resources.get(usize::from(group)))
            .ok_or_else(invariant_error)?;
        let phase = u8::from(resource.access == Access::Consume);
        let mut checked = state.owner_arguments(owner, phase)?;
        checked[2] = arguments[index];
        let valid = state.helper(ledger::LOOKUP, &checked)?;
        let route = BoundaryExitKind::ForeignFailure(FailureRoute::HostAbiFailure);
        cleanup::check_or_exit(state, valid, effect, &route, 3, Some(operation), None)?;
    }
    let FlowStep::Call { outputs, .. } = effect.operation() else {
        return Err(invariant_error());
    };
    let mut snapshots = Vec::new();
    for token in outputs {
        snapshots.push((*token, state.slot_value(*token)?));
        let flag = *state.initialized.get(token).ok_or_else(invariant_error)?;
        let zero = state.builder.ins().iconst(types::I32, 0);
        state.builder.ins().stack_store(types::I64, zero, flag, 0);
    }
    state.snapshots.insert(call, snapshots);
    let reference = *state.environment.imports.get(&operation).ok_or_else(invariant_error)?;
    let instruction = state.builder.ins().call(reference, &arguments);
    let result = if declaration.result == AbiType::Unit {
        state.builder.ins().iconst(types::I32, 0)
    } else {
        *state.builder.inst_results(instruction).first().ok_or_else(invariant_error)?
    };
    state.calls.insert(call, (operation, result));
    let FlowStep::Call { expression, .. } = effect.operation() else {
        return Err(invariant_error());
    };
    state.set(*expression, result)
}

pub(super) fn classify(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
    call: usize,
) -> Result<(), Diagnostic> {
    let (operation, status) = *state.calls.get(&call).ok_or_else(invariant_error)?;
    let FlowStep::Call { recoverable, .. } = effect.operation() else {
        return Err(invariant_error());
    };
    let mut known = state.builder.ins().icmp_imm_s(IntCC::Equal, status, 0);
    for code in recoverable {
        let matches = state.builder.ins().icmp_imm_s(IntCC::Equal, status, i64::from(*code));
        known = state.builder.ins().bor(known, matches);
    }
    let route = BoundaryExitKind::ForeignFailure(FailureRoute::HostAbiFailure);
    cleanup::check_or_exit(state, known, effect, &route, 3, Some(operation), Some(status))?;
    let success = state.builder.ins().icmp_imm_s(IntCC::Equal, status, 0);
    let nonzero = state.builder.create_block();
    let next = state.builder.create_block();
    state.builder.ins().brif(success, next, &[], nonzero, &[]);
    state.builder.switch_to_block(nonzero);
    let snapshots = state.snapshots.get(&call).ok_or_else(invariant_error)?.clone();
    for (token, before) in snapshots {
        let after = state.slot_value(token)?;
        let unchanged = state.builder.ins().icmp(IntCC::Equal, before, after);
        cleanup::check_or_exit(state, unchanged, effect, &route, 3, Some(operation), Some(status))?;
    }
    state.builder.ins().jump(next, &[]);
    state.builder.switch_to_block(next);
    Ok(())
}

pub(super) fn register(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
    call: usize,
    owners: &[usize],
) -> Result<(), Diagnostic> {
    let (operation, status) = *state.calls.get(&call).ok_or_else(invariant_error)?;
    let declaration = state
        .environment
        .program
        .operations()
        .nth(operation)
        .ok_or_else(invariant_error)?
        .declaration();
    let success = state.builder.ins().icmp_imm_s(IntCC::Equal, status, 0);
    let entered = state.builder.create_block();
    let next = state.builder.create_block();
    state.builder.ins().brif(success, entered, &[], next, &[]);
    state.builder.switch_to_block(entered);
    let FlowStep::Call { outputs, .. } = effect.operation() else {
        return Err(invariant_error());
    };
    for (position, owner) in owners.iter().enumerate() {
        let resource = declaration
            .resources
            .iter()
            .filter(|resource| resource.fresh)
            .nth(position)
            .ok_or_else(invariant_error)?;
        let argument = usize::from(*resource.slots.first().ok_or_else(invariant_error)?);
        let token_index = declaration.parameters[..argument]
            .iter()
            .filter(|parameter| matches!(parameter.abi, AbiType::HandleOut | AbiType::I32Out))
            .count();
        let token = *outputs.get(token_index).ok_or_else(invariant_error)?;
        let pointer = state.slot_value(token)?;
        let nonnull = state.builder.ins().icmp_imm_s(IntCC::NotEqual, pointer, 0);
        let present = state.builder.create_block();
        let empty = state.builder.create_block();
        state.builder.ins().brif(nonnull, present, &[], empty, &[]);
        state.builder.switch_to_block(present);
        let function = state.constant(state.environment.ordinal)?;
        let owner_id = state.constant(*owner)?;
        let release = super::super::admit::release_for(
            state.environment.function,
            state.environment.program,
            *owner,
        )
        .ok_or_else(invariant_error)?;
        let release = state.constant(release)?;
        let record = state
            .helper(ledger::REGISTER, &[state.context, pointer, function, owner_id, release])?;
        let accepted = state.builder.ins().icmp_imm_s(IntCC::NotEqual, record, 0);
        let route = BoundaryExitKind::ForeignFailure(FailureRoute::HostAbiFailure);
        cleanup::check_or_exit(state, accepted, effect, &route, 3, Some(operation), Some(status))?;
        let slot = *state.owners.get(owner).ok_or_else(invariant_error)?;
        state.builder.ins().stack_store(types::I64, record, slot, 0);
        let pointer_slot = *state.owner_pointers.get(owner).ok_or_else(invariant_error)?;
        state.builder.ins().stack_store(types::I64, pointer, pointer_slot, 0);
        state.builder.ins().jump(empty, &[]);
        state.builder.switch_to_block(empty);
    }
    state.builder.ins().jump(next, &[]);
    state.builder.switch_to_block(next);
    Ok(())
}

pub(super) fn settle(
    state: &mut State<'_, '_>,
    call: usize,
    maximum: usize,
) -> Result<(), Diagnostic> {
    // Registration consumed one credit per confirmed non-null acquisition. The remaining credit
    // for this call is returned only after status and recoverable failure atomicity were checked.
    let mut used = state.builder.ins().iconst(types::I32, 0);
    for effect in state.environment.function.effects() {
        if let FlowStep::Call { call: original, created_owners, .. } = effect.operation() {
            if *original != call {
                continue;
            }
            for owner in created_owners {
                let record = state.record(*owner)?;
                let present = state.builder.ins().icmp_imm_s(IntCC::NotEqual, record, 0);
                let one = state.builder.ins().iconst(types::I32, 1);
                let zero = state.builder.ins().iconst(types::I32, 0);
                let count = state.builder.ins().select(present, one, zero);
                used = state.builder.ins().iadd(used, count);
            }
        }
    }
    let maximum = state.constant(maximum)?;
    let unused = state.builder.ins().isub(maximum, used);
    let reserved = state.builder.ins().load(types::I32, MemFlagsData::new(), state.context, 16);
    let remaining = state.builder.ins().isub(reserved, unused);
    state.builder.ins().store(MemFlagsData::new(), remaining, state.context, 16);
    Ok(())
}
