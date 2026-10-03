//! Exact private release, unresolved obligations and cleanup-before-result transfer.

use super::super::{super::invariant_error, state::State};
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Value, condcodes::IntCC, types};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::contract::{PrivateOrigin, PrivateOwner, ValueType};
use zryna_ownership_runtime_abi::LogicalOperation;

pub(in crate::native_c_v0::resources) fn address(
    state: &mut State<'_, '_>,
    origin: PrivateOrigin,
) -> Result<Value, Diagnostic> {
    let (handle, _) = *state.private.get(&origin).ok_or_else(invariant_error)?;
    Ok(state.builder.ins().stack_addr(types::I64, handle, 0))
}

pub(in crate::native_c_v0::resources) fn host_check(
    state: &mut State<'_, '_>,
    valid: Value,
) -> Result<(), Diagnostic> {
    let next = state.builder.create_block();
    let failed = state.builder.create_block();
    state.builder.ins().brif(valid, next, &[], failed, &[]);
    state.builder.switch_to_block(failed);
    state.finish(3, None, None, 0, None)?;
    state.builder.switch_to_block(next);
    Ok(())
}

pub(in crate::native_c_v0::resources) fn release(
    state: &mut State<'_, '_>,
    owner: &PrivateOwner,
) -> Result<(), Diagnostic> {
    let (handle, active) = *state.private.get(&owner.origin).ok_or_else(invariant_error)?;
    let live = state.builder.ins().stack_load(types::I64, types::I32, active, 0);
    let next = state.builder.create_block();
    let entered = state.builder.create_block();
    state.builder.ins().brif(live, entered, &[], next, &[]);
    state.builder.switch_to_block(entered);
    let abi = state.environment.program.source().runtime_abi();
    let logical = abi
        .operations()
        .find(|operation| operation.id() == owner.release)
        .ok_or_else(invariant_error)?
        .operation();
    let symbol = abi
        .native_linux_x86_64_functions()
        .find(|function| function.operation() == owner.release)
        .ok_or_else(invariant_error)?
        .symbol();
    let address = state.builder.ins().stack_addr(types::I64, handle, 0);
    let arguments = match logical {
        LogicalOperation::Release => {
            let pointer = state.builder.ins().stack_load(types::I64, types::I64, handle, 0);
            let length = state.builder.ins().stack_load(types::I64, types::I64, handle, 8);
            let alignment = state.constant(
                usize::try_from(owner.packed_alignment.ok_or_else(invariant_error)?)
                    .map_err(|_| invariant_error())?,
            )?;
            vec![pointer, length, alignment]
        }
        LogicalOperation::StringRelease => vec![address],
        LogicalOperation::VecReleaseStorage => {
            let record = state
                .environment
                .program
                .source()
                .native_layouts()
                .type_by_id(owner.ty.ok_or_else(invariant_error)?)
                .ok_or_else(invariant_error)?;
            let element = record.referenced_type().ok_or_else(invariant_error)?;
            let element =
                state.constant(usize::try_from(element.index()).map_err(|_| invariant_error())?)?;
            vec![element, address]
        }
        _ => return Err(invariant_error()),
    };
    let status = state.helper(symbol, &arguments)?;
    let valid = state.builder.ins().icmp_imm_s(IntCC::Equal, status, 0);
    // Failure overrides the pending completion. Active flags and all suffix owners remain;
    // there is no guessed retry or claim that their physical release occurred.
    host_check(state, valid)?;
    let zero = state.builder.ins().iconst(types::I32, 0);
    state.builder.ins().stack_store(types::I64, zero, active, 0);
    state.builder.ins().jump(next, &[]);
    state.builder.switch_to_block(next);
    Ok(())
}

pub(in crate::native_c_v0::resources) fn confirm_empty(
    state: &mut State<'_, '_>,
    owner: usize,
) -> Result<bool, Diagnostic> {
    if !super::foreign_bytes(state, owner)? {
        return Ok(false);
    }
    let record = state.record(owner)?;
    let nonempty = state.builder.ins().icmp_imm_s(IntCC::NotEqual, record, 0);
    let next = state.builder.create_block();
    let entered = state.builder.create_block();
    state.builder.ins().brif(nonempty, entered, &[], next, &[]);
    state.builder.switch_to_block(entered);
    let arguments = state.owner_arguments(owner, 2)?;
    let valid = state.helper(super::super::ledger::LOOKUP, &arguments)?;
    host_check(state, valid)?;
    let valid = state.helper(super::super::ledger::CONFIRM, &[state.context, arguments[1]])?;
    host_check(state, valid)?;
    let null = state.builder.ins().iconst(types::I64, 0);
    let slot = *state.owners.get(&owner).ok_or_else(invariant_error)?;
    state.builder.ins().stack_store(types::I64, null, slot, 0);
    state.builder.ins().jump(next, &[]);
    state.builder.switch_to_block(next);
    Ok(true)
}

pub(in crate::native_c_v0::resources) fn finish(
    state: &mut State<'_, '_>,
    tag: Value,
    value: Option<Value>,
    protected_result: Option<PrivateOrigin>,
) -> Result<Value, Diagnostic> {
    let zero = state.builder.ins().iconst(types::I32, 0);
    if state.environment.byte_channel
        && matches!(state.environment.function.result().1, ValueType::VecI32 | ValueType::String)
        && value.is_some()
    {
        // Transfer the origin authenticated by this terminal edge, including conditional returns.
        let origin = protected_result.ok_or_else(invariant_error)?;
        let (handle, active) = *state.private.get(&origin).ok_or_else(invariant_error)?;
        let returned = state.builder.ins().icmp_imm_s(IntCC::Equal, tag, 0);
        let null = state.builder.ins().iconst(types::I64, 0);
        for offset in [0, 8, 16] {
            let field = state.builder.ins().stack_load(types::I64, types::I64, handle, offset);
            let exposed = state.builder.ins().select(returned, field, null);
            state.builder.ins().store(MemFlagsData::new(), exposed, state.outcome, 32 + offset);
        }
        let prior = state.builder.ins().stack_load(types::I64, types::I32, active, 0);
        let retained = state.builder.ins().select(returned, zero, prior);
        state.builder.ins().stack_store(types::I64, retained, active, 0);
    }
    let mut count = zero;
    for (_, active) in state.private.values() {
        let live = state.builder.ins().stack_load(types::I64, types::I32, *active, 0);
        count = state.builder.ins().iadd(count, live);
    }
    if state.environment.byte_channel {
        let prior = state.builder.ins().load(types::I32, MemFlagsData::new(), state.context, 1560);
        count = state.builder.ins().iadd(count, prior);
        state.builder.ins().store(MemFlagsData::new(), count, state.context, 1560);
    }
    Ok(count)
}

pub(in crate::native_c_v0::resources) fn unresolved(
    state: &mut State<'_, '_>,
    private_count: Value,
) {
    let foreign = state.builder.ins().load(types::I32, MemFlagsData::new(), state.outcome, 20);
    let count = state.builder.ins().iadd(foreign, private_count);
    state.builder.ins().store(MemFlagsData::new(), count, state.outcome, 20);
}

pub(in crate::native_c_v0::resources) fn rejected(state: &mut State<'_, '_>) {
    if !state.environment.byte_channel {
        return;
    }
    let done = state.builder.create_block();
    let valid = state.builder.create_block();
    let context = state.builder.ins().icmp_imm_s(IntCC::NotEqual, state.context, 0);
    let outcome = state.builder.ins().icmp_imm_s(IntCC::NotEqual, state.outcome, 0);
    let present = state.builder.ins().band(context, outcome);
    state.builder.ins().brif(present, valid, &[], done, &[]);
    state.builder.switch_to_block(valid);
    let context_low = state.builder.ins().band_imm_u(state.context, 7);
    let outcome_low = state.builder.ins().band_imm_u(state.outcome, 7);
    let low = state.builder.ins().bor(context_low, outcome_low);
    let aligned = state.builder.ins().icmp_imm_s(IntCC::Equal, low, 0);
    let valid = state.builder.create_block();
    state.builder.ins().brif(aligned, valid, &[], done, &[]);
    state.builder.switch_to_block(valid);
    let magic = state.builder.ins().load(types::I64, MemFlagsData::new(), state.context, 0);
    let expected = state.builder.ins().iconst(types::I64, super::MAGIC);
    let exact = state.builder.ins().icmp(IntCC::Equal, magic, expected);
    let valid = state.builder.create_block();
    state.builder.ins().brif(exact, valid, &[], done, &[]);
    state.builder.switch_to_block(valid);
    let prior = state.builder.ins().load(types::I32, MemFlagsData::new(), state.context, 1560);
    unresolved(state, prior);
    let null = state.builder.ins().iconst(types::I64, 0);
    for offset in [32, 40, 48] {
        state.builder.ins().store(MemFlagsData::new(), null, state.outcome, offset);
    }
    state.builder.ins().jump(done, &[]);
    state.builder.switch_to_block(done);
}
