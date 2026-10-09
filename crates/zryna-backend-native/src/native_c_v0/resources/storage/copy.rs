//! Registered foreign bytes remain distinct from newly initialized private Vec storage.

use super::{
    super::{super::invariant_error, cleanup, ledger, state::State},
    allocation_status,
    loans::loop_values,
    owners::{address, host_check},
};
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Value, condcodes::IntCC, types};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::{
    VerifiedEffect,
    contract::{
        AbiType, BoundaryExitKind, Encoding, FailureRoute, FlowStep, NullRule, PrivateCopy,
        StorageStage,
    },
};

pub(in crate::native_c_v0::resources) struct BytesPolicy {
    pub(in crate::native_c_v0::resources) expected_argument: Option<usize>,
    pub(in crate::native_c_v0::resources) count: usize,
    pub(in crate::native_c_v0::resources) maximum: u32,
    pub(in crate::native_c_v0::resources) null_rule: NullRule,
    pub(in crate::native_c_v0::resources) encoding: Encoding,
}

pub(in crate::native_c_v0::resources) fn policy(
    state: &State<'_, '_>,
    owner: usize,
) -> Result<Option<BytesPolicy>, Diagnostic> {
    for effect in state.environment.function.effects() {
        let FlowStep::Call { operation, created_owners, outputs, .. } = effect.operation() else {
            continue;
        };
        let Some(position) = created_owners.iter().position(|id| *id == owner) else {
            continue;
        };
        let declaration = state
            .environment
            .program
            .operations()
            .nth(*operation)
            .ok_or_else(invariant_error)?
            .declaration();
        let resource = declaration
            .resources
            .iter()
            .filter(|resource| resource.fresh)
            .nth(position)
            .ok_or_else(invariant_error)?;
        if declaration.parameters[usize::from(resource.slots[0])].abi != AbiType::BytesOwnedOut {
            return Ok(None);
        }
        let count_argument = usize::from(*resource.slots.get(1).ok_or_else(invariant_error)?);
        let index = declaration.parameters[..count_argument]
            .iter()
            .filter(|parameter| {
                matches!(
                    parameter.abi,
                    AbiType::I32Out
                        | AbiType::HandleOut
                        | AbiType::BytesOwnedOut
                        | AbiType::CountOut
                )
            })
            .count();
        return Ok(Some(BytesPolicy {
            expected_argument: resource.expected_length_slot.map(usize::from),
            count: *outputs.get(index).ok_or_else(invariant_error)?,
            maximum: resource.max_bytes.ok_or_else(invariant_error)?,
            null_rule: resource.null_rule,
            encoding: resource.encoding,
        }));
    }
    Err(invariant_error())
}

pub(in crate::native_c_v0::resources) fn foreign_bytes(
    state: &State<'_, '_>,
    owner: usize,
) -> Result<bool, Diagnostic> {
    Ok(policy(state, owner)?.is_some())
}

pub(in crate::native_c_v0::resources) fn length(
    state: &mut State<'_, '_>,
    owner: usize,
) -> Result<Value, Diagnostic> {
    let slot = *state.owner_lengths.get(&owner).ok_or_else(invariant_error)?;
    Ok(state.builder.ins().stack_load(types::I64, types::I64, slot, 0))
}

pub(in crate::native_c_v0::resources) fn pointer(
    state: &mut State<'_, '_>,
    owner: usize,
) -> Result<Value, Diagnostic> {
    let slot = *state.owner_pointers.get(&owner).ok_or_else(invariant_error)?;
    Ok(state.builder.ins().stack_load(types::I64, types::I64, slot, 0))
}

pub(in crate::native_c_v0::resources) fn valid_metadata(
    state: &mut State<'_, '_>,
    owner: usize,
    policy: &BytesPolicy,
) -> Result<Value, Diagnostic> {
    let pointer = pointer(state, owner)?;
    let length = length(state, owner)?;
    let empty = state.builder.ins().icmp_imm_s(IntCC::Equal, length, 0);
    let absent = state.builder.ins().icmp_imm_s(IntCC::Equal, pointer, 0);
    let paired = match policy.null_rule {
        NullRule::NullZero => state.builder.ins().icmp(IntCC::Equal, empty, absent),
        NullRule::Nonnull => state.builder.ins().icmp_imm_s(IntCC::NotEqual, pointer, 0),
    };
    let bounded = state.builder.ins().icmp_imm_u(
        IntCC::UnsignedLessThanOrEqual,
        length,
        i64::from(policy.maximum),
    );
    let mut valid = state.builder.ins().band(paired, bounded);
    if policy.expected_argument.is_some() {
        let slot = *state.owner_expected.get(&owner).ok_or_else(invariant_error)?;
        let expected = state.builder.ins().stack_load(types::I64, types::I64, slot, 0);
        let exact = state.builder.ins().icmp(IntCC::Equal, length, expected);
        valid = state.builder.ins().band(valid, exact);
    }
    Ok(valid)
}

pub(in crate::native_c_v0::resources) fn take(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
    owner: usize,
) -> Result<bool, Diagnostic> {
    let Some(policy) = policy(state, owner)? else {
        return Ok(false);
    };
    let route = BoundaryExitKind::ForeignFailure(FailureRoute::HostAbiFailure);
    let valid = valid_metadata(state, owner, &policy)?;
    cleanup::check_or_exit(state, valid, effect, &route, 3, None, None)?;
    let pointer = pointer(state, owner)?;
    let length = length(state, owner)?;
    let record = state.record(owner)?;
    let empty = state.builder.ins().icmp_imm_s(IntCC::Equal, pointer, 0);
    let entered = state.builder.create_block();
    let next = state.builder.create_block();
    state.builder.ins().brif(empty, next, &[], entered, &[]);
    state.builder.switch_to_block(entered);
    let arguments = state.owner_arguments(owner, 0)?;
    let valid = state.helper(ledger::LOOKUP, &arguments)?;
    cleanup::check_or_exit(state, valid, effect, &route, 3, None, None)?;
    if policy.encoding == Encoding::Utf8 {
        let valid = state.helper(super::UTF8, &[pointer, length])?;
        cleanup::check_or_exit(state, valid, effect, &route, 3, None, None)?;
    }
    state.builder.ins().jump(next, &[]);
    state.builder.switch_to_block(next);
    let canonical = state.builder.ins().icmp_imm_s(IntCC::Equal, record, 0);
    let coherent = state.builder.ins().icmp(IntCC::Equal, empty, canonical);
    cleanup::check_or_exit(state, coherent, effect, &route, 3, None, None)?;
    let FlowStep::Take { expression, .. } = effect.operation() else {
        return Err(invariant_error());
    };
    state.set(*expression, pointer)?;
    Ok(true)
}

pub(in crate::native_c_v0::resources) fn stage(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
    copy: &PrivateCopy,
    action: StorageStage,
) -> Result<(), Diagnostic> {
    let pointer = pointer(state, copy.foreign_owner)?;
    let length = length(state, copy.foreign_owner)?;
    let output = address(state, copy.result)?;
    match action {
        StorageStage::ValidateForeign => {
            let policy = policy(state, copy.foreign_owner)?.ok_or_else(invariant_error)?;
            let valid = valid_metadata(state, copy.foreign_owner, &policy)?;
            host_check(state, valid)?;
        }
        StorageStage::CheckedCapacity => {
            let maximum = 1_048_576_u64.min(2_147_483_647_u64 / copy.stride);
            let valid = state.builder.ins().icmp_imm_u(
                IntCC::UnsignedLessThanOrEqual,
                length,
                i64::try_from(maximum).map_err(|_| invariant_error())?,
            );
            let kind = effect
                .exits()
                .iter()
                .find_map(|exit| {
                    if let BoundaryExitKind::PrivateTrap(fault) = &exit.kind
                        && fault.declaration.status()
                            == zryna_ownership_runtime_abi::RuntimeStatus::Capacity
                    {
                        Some(exit.kind.clone())
                    } else {
                        None
                    }
                })
                .ok_or_else(invariant_error)?;
            cleanup::check_or_exit(state, valid, effect, &kind, 2, None, None)?;
        }
        StorageStage::AllocateNonempty => {
            let entered = state.builder.create_block();
            let next = state.builder.create_block();
            let nonempty = state.builder.ins().icmp_imm_s(IntCC::NotEqual, length, 0);
            state.builder.ins().brif(nonempty, entered, &[], next, &[]);
            state.builder.switch_to_block(entered);
            let symbol = state
                .environment
                .program
                .source()
                .runtime_abi()
                .native_linux_x86_64_functions()
                .find(|function| function.operation() == copy.allocation)
                .ok_or_else(invariant_error)?
                .symbol();
            let element = state.constant(
                usize::try_from(copy.element_type.index()).map_err(|_| invariant_error())?,
            )?;
            let status = state.helper(symbol, &[element, length, output])?;
            allocation_status(state, effect, status)?;
            let pointer = state.builder.ins().load(types::I64, MemFlagsData::new(), output, 0);
            let (_, active) = *state.private.get(&copy.result).ok_or_else(invariant_error)?;
            let one = state.builder.ins().iconst(types::I32, 1);
            state.builder.ins().stack_store(types::I64, one, active, 0);
            let present = state.builder.ins().icmp_imm_s(IntCC::NotEqual, pointer, 0);
            host_check(state, present)?;
            let initialized = state.builder.ins().load(types::I64, MemFlagsData::new(), output, 8);
            let zero = state.builder.ins().icmp_imm_s(IntCC::Equal, initialized, 0);
            host_check(state, zero)?;
            let capacity = state.builder.ins().load(types::I64, MemFlagsData::new(), output, 16);
            let exact = state.builder.ins().icmp(IntCC::Equal, capacity, length);
            host_check(state, exact)?;
            state.builder.ins().jump(next, &[]);
            state.builder.switch_to_block(next);
        }
        StorageStage::Initialize => {
            let destination = state.builder.ins().load(types::I64, MemFlagsData::new(), output, 0);
            loop_values(state, length, |state, index| {
                let source = state.builder.ins().iadd(pointer, index);
                let byte = state.builder.ins().load(types::I8, MemFlagsData::new(), source, 0);
                let value = state.builder.ins().uextend(types::I32, byte);
                let offset = state
                    .builder
                    .ins()
                    .imul_imm_u(index, i64::try_from(copy.stride).map_err(|_| invariant_error())?);
                let destination = state.builder.ins().iadd(destination, offset);
                state.builder.ins().store(MemFlagsData::new(), value, destination, 0);
                Ok(())
            })?;
        }
        StorageStage::Commit => {
            state.builder.ins().store(MemFlagsData::new(), length, output, 8);
            state.set(copy.expression, output)?;
        }
        _ => return Err(invariant_error()),
    }
    Ok(())
}
