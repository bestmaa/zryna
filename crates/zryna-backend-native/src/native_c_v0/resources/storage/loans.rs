//! Real byte packing and complete retained String validation in the sealed stage order.

use super::{
    super::{super::invariant_error, cleanup, state::State},
    allocation_status,
    owners::{address, host_check},
};
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Value, condcodes::IntCC, types};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::{
    VerifiedEffect,
    contract::{
        BoundaryCheck, BoundaryExitKind, Encoding, FlowStep, NullRule, PrivateLoan, StorageStage,
        TrapRequirement,
    },
};

pub(in crate::native_c_v0::resources) fn stage(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
    loan: &PrivateLoan,
    action: StorageStage,
) -> Result<(), Diagnostic> {
    let source = address(state, loan.source)?;
    let pointer = state.builder.ins().load(types::I64, MemFlagsData::new(), source, 0);
    let length = state.builder.ins().load(types::I64, MemFlagsData::new(), source, 8);
    match action {
        StorageStage::Utf8 => {
            let valid = state.helper(super::UTF8, &[pointer, length])?;
            // A corrupted compiler-private String violates its issuer; there is no invented
            // source-level UTF8 trap or cleanup edge. Preserve its unresolved private obligation.
            host_check(state, valid)?;
        }
        StorageStage::Length => {
            let maximum = i64::try_from(loan.maximum_bytes).map_err(|_| invariant_error())?;
            let valid =
                state.builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, length, maximum);
            foreign_check(state, effect, valid, TrapRequirement::ForeignLength)?;
            if loan.scratch.is_none() {
                commit(state, loan, pointer, length)?;
            }
        }
        StorageStage::ByteRange => {
            loop_values(state, length, |state, index| {
                let offset = state.builder.ins().imul_imm_u(
                    index,
                    i64::try_from(loan.source_stride).map_err(|_| invariant_error())?,
                );
                let address = state.builder.ins().iadd(pointer, offset);
                let element = state.builder.ins().load(types::I32, MemFlagsData::new(), address, 0);
                let valid =
                    state.builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, element, 255);
                foreign_check(state, effect, valid, TrapRequirement::ForeignByteRange)
            })?;
        }
        StorageStage::AllocateNonempty => {
            let origin = loan.scratch.ok_or_else(invariant_error)?;
            let output = address(state, origin)?;
            let entered = state.builder.create_block();
            let next = state.builder.create_block();
            let nonempty = state.builder.ins().icmp_imm_s(IntCC::NotEqual, length, 0);
            state.builder.ins().brif(nonempty, entered, &[], next, &[]);
            state.builder.switch_to_block(entered);
            let abi = state.environment.program.source().runtime_abi();
            let operation = loan.allocation.ok_or_else(invariant_error)?;
            let symbol = abi
                .native_linux_x86_64_functions()
                .find(|function| function.operation() == operation)
                .ok_or_else(invariant_error)?
                .symbol();
            let alignment = state.constant(
                usize::try_from(loan.backing_alignment).map_err(|_| invariant_error())?,
            )?;
            let status = state.helper(symbol, &[length, alignment, output])?;
            allocation_status(state, effect, status)?;
            let pointer = state.builder.ins().load(types::I64, MemFlagsData::new(), output, 0);
            let (_, active) = *state.private.get(&origin).ok_or_else(invariant_error)?;
            let one = state.builder.ins().iconst(types::I32, 1);
            state.builder.ins().stack_store(types::I64, one, active, 0);
            let valid = state.builder.ins().icmp_imm_s(IntCC::NotEqual, pointer, 0);
            host_check(state, valid)?;
            state.builder.ins().store(MemFlagsData::new(), length, output, 8);
            state.builder.ins().store(MemFlagsData::new(), length, output, 16);
            state.builder.ins().jump(next, &[]);
            state.builder.switch_to_block(next);
        }
        StorageStage::Initialize => {
            let output = address(state, loan.scratch.ok_or_else(invariant_error)?)?;
            let destination = state.builder.ins().load(types::I64, MemFlagsData::new(), output, 0);
            loop_values(state, length, |state, index| {
                let offset = state.builder.ins().imul_imm_u(
                    index,
                    i64::try_from(loan.source_stride).map_err(|_| invariant_error())?,
                );
                let input = state.builder.ins().iadd(pointer, offset);
                let value = state.builder.ins().load(types::I32, MemFlagsData::new(), input, 0);
                let byte = state.builder.ins().ireduce(types::I8, value);
                let output = state.builder.ins().iadd(destination, index);
                state.builder.ins().store(MemFlagsData::new(), byte, output, 0);
                Ok(())
            })?;
        }
        StorageStage::Commit => {
            let output = address(state, loan.scratch.ok_or_else(invariant_error)?)?;
            let pointer = state.builder.ins().load(types::I64, MemFlagsData::new(), output, 0);
            commit(state, loan, pointer, length)?;
        }
        _ => return Err(invariant_error()),
    }
    Ok(())
}

fn commit(
    state: &mut State<'_, '_>,
    loan: &PrivateLoan,
    pointer: Value,
    length: Value,
) -> Result<(), Diagnostic> {
    let slot = *state.loans.get(&loan.token).ok_or_else(invariant_error)?;
    // Zero length always exposes canonical null/zero, including retained source with spare capacity.
    let empty = state.builder.ins().icmp_imm_s(IntCC::Equal, length, 0);
    let null = state.builder.ins().iconst(types::I64, 0);
    let pointer = state.builder.ins().select(empty, null, pointer);
    state.builder.ins().stack_store(types::I64, pointer, slot, 0);
    state.builder.ins().stack_store(types::I64, length, slot, 8);
    state.set(loan.expression, pointer)
}

fn foreign_check(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
    valid: Value,
    trap: TrapRequirement,
) -> Result<(), Diagnostic> {
    cleanup::check_or_exit(
        state,
        valid,
        effect,
        &BoundaryExitKind::ForeignTrap(trap),
        2,
        None,
        None,
    )
}

pub(in crate::native_c_v0::resources) fn loop_values(
    state: &mut State<'_, '_>,
    length: Value,
    mut body: impl FnMut(&mut State<'_, '_>, Value) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    let header = state.builder.create_block();
    state.builder.append_block_param(header, types::I64);
    let entered = state.builder.create_block();
    let done = state.builder.create_block();
    let zero = state.builder.ins().iconst(types::I64, 0);
    state.builder.ins().jump(header, &[zero.into()]);
    state.builder.switch_to_block(header);
    let index = state.builder.block_params(header)[0];
    let remains = state.builder.ins().icmp(IntCC::UnsignedLessThan, index, length);
    state.builder.ins().brif(remains, entered, &[], done, &[]);
    state.builder.switch_to_block(entered);
    body(state, index)?;
    let next = state.builder.ins().iadd_imm_u(index, 1);
    state.builder.ins().jump(header, &[next.into()]);
    state.builder.switch_to_block(done);
    Ok(())
}

pub(in crate::native_c_v0::resources) fn byte_length(
    state: &mut State<'_, '_>,
    expression: usize,
) -> Result<Value, Diagnostic> {
    let token = state
        .environment
        .function
        .values()
        .get(expression)
        .and_then(|value| value.token)
        .ok_or_else(invariant_error)?;
    let slot = *state.loans.get(&token).ok_or_else(invariant_error)?;
    let length = state.builder.ins().stack_load(types::I64, types::I64, slot, 8);
    Ok(state.builder.ins().ireduce(types::I32, length))
}

pub(in crate::native_c_v0::resources) fn boundary(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
    check: &BoundaryCheck,
) -> Result<(), Diagnostic> {
    let valid = match check {
        BoundaryCheck::CountConversion { expression } => {
            let value = state.value(*expression)?;
            state.builder.ins().icmp_imm_s(IntCC::SignedGreaterThanOrEqual, value, 0)
        }
        BoundaryCheck::Borrow { group, loan, count_expression, .. } => {
            let slot = *state.loans.get(loan).ok_or_else(invariant_error)?;
            let length = state.builder.ins().stack_load(types::I64, types::I64, slot, 8);
            let count = state.value(*count_expression)?;
            let count = state.builder.ins().uextend(types::I64, count);
            let within = state.builder.ins().icmp(IntCC::UnsignedLessThanOrEqual, count, length);
            let FlowStep::Call { operation, .. } = effect.operation() else {
                return Err(invariant_error());
            };
            let declaration = state
                .environment
                .program
                .operations()
                .nth(*operation)
                .ok_or_else(invariant_error)?
                .declaration();
            let resource = declaration.resources.get(*group).ok_or_else(invariant_error)?;
            let maximum = resource.max_bytes.ok_or_else(invariant_error)?;
            let bounded = state.builder.ins().icmp_imm_u(
                IntCC::UnsignedLessThanOrEqual,
                count,
                i64::from(maximum),
            );
            let within = state.builder.ins().band(within, bounded);
            let pointer = state.builder.ins().stack_load(types::I64, types::I64, slot, 0);
            let present = state.builder.ins().icmp_imm_s(IntCC::NotEqual, pointer, 0);
            let empty = state.builder.ins().icmp_imm_s(IntCC::Equal, count, 0);
            let nullable = if resource.null_rule == NullRule::NullZero {
                state.builder.ins().bor(present, empty)
            } else {
                present
            };
            let valid = state.builder.ins().band(within, nullable);
            cleanup::check_or_exit(
                state,
                valid,
                effect,
                &BoundaryExitKind::ForeignBoundaryFailure(check.clone()),
                3,
                None,
                None,
            )?;
            if resource.encoding == Encoding::Utf8 {
                let utf8 = state.helper(super::UTF8, &[pointer, count])?;
                cleanup::check_or_exit(
                    state,
                    utf8,
                    effect,
                    &BoundaryExitKind::ForeignBoundaryFailure(check.clone()),
                    3,
                    None,
                    None,
                )?;
            }
            valid
        }
        _ => return Err(invariant_error()),
    };
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

pub(in crate::native_c_v0::resources) fn end_loans(
    state: &mut State<'_, '_>,
    loans: &[usize],
) -> Result<(), Diagnostic> {
    let null = state.builder.ins().iconst(types::I64, 0);
    for token in loans {
        let slot = *state.loans.get(token).ok_or_else(invariant_error)?;
        for offset in [0, 8] {
            state.builder.ins().stack_store(types::I64, null, slot, offset);
        }
    }
    Ok(())
}
