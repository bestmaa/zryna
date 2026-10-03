//! Original expression/statement order plus the independently verified physical effect sequence.

mod calls;
mod effects;
mod values;

use super::{
    super::invariant_error,
    ledger,
    state::{Environment, State},
};
use cranelift_codegen::{
    Context,
    ir::{InstBuilder, MemFlagsData, condcodes::IntCC, types},
};
use cranelift_frontend::FunctionBuilderContext;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::contract::{SourceType, StatementKind};

pub(super) fn build(
    environment: Environment<'_>,
    context: &mut Context,
    frontend: &mut FunctionBuilderContext,
    config: cranelift_codegen::isa::TargetFrontendConfig,
) -> Result<(), Diagnostic> {
    let mut state = State::new(environment, context, frontend)?;
    let arity = state.constant(state.environment.function.parameters().len())?;
    let valid =
        state.helper(ledger::ENTER, &[state.context, state.inputs, state.outcome, arity])?;
    let good = state.builder.create_block();
    let bad = state.builder.create_block();
    state.builder.ins().brif(valid, good, &[], bad, &[]);
    state.builder.switch_to_block(bad);
    super::storage::rejected(&mut state);
    let tag = state.builder.ins().iconst(types::I32, 3);
    state.builder.ins().return_(&[tag]);
    state.builder.switch_to_block(good);
    let zero = state.builder.ins().iconst(types::I32, 0);
    let null = state.builder.ins().iconst(types::I64, 0);
    for slot in state.initialized.values() {
        state.builder.ins().stack_store(types::I64, zero, *slot, 0);
    }
    for slot in state
        .owners
        .values()
        .chain(state.owner_pointers.values())
        .chain(state.owner_lengths.values())
        .chain(state.owner_expected.values())
    {
        state.builder.ins().stack_store(types::I64, null, *slot, 0);
    }
    super::storage::parameters(&mut state)?;
    for (index, parameter) in state.environment.function.bindings().iter().enumerate() {
        if matches!(parameter.ty, SourceType::String | SourceType::VecI32) {
            let origin = zryna_native_mir::native_c_v0::contract::PrivateOrigin::Parameter(index);
            let value = super::storage::address(&mut state, origin)?;
            state.locals.push(value);
            continue;
        }
        let offset = index
            .checked_mul(4)
            .and_then(|value| value.checked_add(8))
            .and_then(|value| i32::try_from(value).ok())
            .ok_or_else(invariant_error)?;
        let value = state.builder.ins().load(types::I32, MemFlagsData::new(), state.inputs, offset);
        if parameter.ty == SourceType::Bool {
            let valid = state.builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, value, 1);
            let next = state.builder.create_block();
            let fail = state.builder.create_block();
            state.builder.ins().brif(valid, next, &[], fail, &[]);
            state.builder.switch_to_block(fail);
            state.finish(3, None, None, 0, None)?;
            state.builder.switch_to_block(next);
        }
        state.locals.push(value);
    }
    for statement in state.environment.function.statements() {
        let expression = match &statement.kind {
            StatementKind::Const(_, expression)
            | StatementKind::Return(expression)
            | StatementKind::Expression(expression)
            | StatementKind::Guard(_, expression) => *expression,
        };
        values::through(&mut state, expression)?;
        if let StatementKind::Const(_, _) = statement.kind {
            state.locals.push(state.value(expression)?);
        }
    }
    if !state.terminated || state.cursor != state.environment.function.values().len() {
        return Err(invariant_error());
    }
    state.builder.seal_all_blocks();
    state.builder.finalize(config);
    Ok(())
}
