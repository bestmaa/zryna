//! Admit compiler-private owned inputs before transferring any parameter obligation.

use super::{
    super::{super::invariant_error, state::State},
    owners::host_check,
};
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, condcodes::IntCC, types};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::contract::{PrivateOrigin, SourceType};

pub(in crate::native_c_v0::resources) fn parameters(
    state: &mut State<'_, '_>,
) -> Result<(), Diagnostic> {
    let zero = state.builder.ins().iconst(types::I32, 0);
    let null = state.builder.ins().iconst(types::I64, 0);
    for (handle, active) in state.private.values() {
        state.builder.ins().stack_store(types::I64, zero, *active, 0);
        for offset in [0, 8, 16] {
            state.builder.ins().stack_store(types::I64, null, *handle, offset);
        }
    }
    for loan in state.loans.values() {
        for offset in [0, 8] {
            state.builder.ins().stack_store(types::I64, null, *loan, offset);
        }
    }
    if !state.environment.byte_channel {
        return Ok(());
    }
    for pointer in [state.inputs, state.outcome] {
        let low = state.builder.ins().band_imm_u(pointer, 7);
        let aligned = state.builder.ins().icmp_imm_s(IntCC::Equal, low, 0);
        host_check(state, aligned)?;
    }
    for offset in [32, 40, 48] {
        state.builder.ins().store(MemFlagsData::new(), null, state.outcome, offset);
    }
    for offset in [1560, 1564] {
        let field =
            state.builder.ins().load(types::I32, MemFlagsData::new(), state.context, offset);
        let empty = state.builder.ins().icmp_imm_s(IntCC::Equal, field, 0);
        host_check(state, empty)?;
    }
    let mut pointers = Vec::new();
    for (index, binding) in state.environment.function.bindings().iter().enumerate() {
        if !matches!(binding.ty, SourceType::String | SourceType::VecI32) {
            continue;
        }
        let offset = i32::try_from(72 + index * 24).map_err(|_| invariant_error())?;
        let pointer =
            state.builder.ins().load(types::I64, MemFlagsData::new(), state.inputs, offset);
        let length =
            state.builder.ins().load(types::I64, MemFlagsData::new(), state.inputs, offset + 8);
        let capacity =
            state.builder.ins().load(types::I64, MemFlagsData::new(), state.inputs, offset + 16);
        let bounded = state.builder.ins().icmp(IntCC::UnsignedLessThanOrEqual, length, capacity);
        host_check(state, bounded)?;
        let maximum = if binding.ty == SourceType::String { 2_147_483_647 } else { 1_048_576 };
        let bounded =
            state.builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, capacity, maximum);
        host_check(state, bounded)?;
        let empty = state.builder.ins().icmp_imm_s(IntCC::Equal, capacity, 0);
        let absent = state.builder.ins().icmp_imm_s(IntCC::Equal, pointer, 0);
        let consistent = state.builder.ins().icmp(IntCC::Equal, empty, absent);
        host_check(state, consistent)?;
        if binding.ty == SourceType::VecI32 {
            let bits = state.builder.ins().band_imm_u(pointer, 3);
            let aligned = state.builder.ins().icmp_imm_s(IntCC::Equal, bits, 0);
            host_check(state, aligned)?;
        }
        for earlier in &pointers {
            let distinct = state.builder.ins().icmp(IntCC::NotEqual, pointer, *earlier);
            let valid = state.builder.ins().bor(distinct, absent);
            host_check(state, valid)?;
        }
        pointers.push(pointer);
        let (handle, _) =
            *state.private.get(&PrivateOrigin::Parameter(index)).ok_or_else(invariant_error)?;
        for (offset, field) in [(0, pointer), (8, length), (16, capacity)] {
            state.builder.ins().stack_store(types::I64, field, handle, offset);
        }
    }
    // Every parameter is structurally admitted before any owned transfer commits. The channel
    // remains compiler-private: these checks do not manufacture backing-memory provenance.
    for (origin, (handle, active)) in &state.private {
        if !matches!(origin, PrivateOrigin::Parameter(_)) {
            continue;
        }
        let pointer = state.builder.ins().stack_load(types::I64, types::I64, *handle, 0);
        let present = state.builder.ins().icmp_imm_s(IntCC::NotEqual, pointer, 0);
        let present = state.builder.ins().uextend(types::I32, present);
        state.builder.ins().stack_store(types::I64, present, *active, 0);
    }
    Ok(())
}
