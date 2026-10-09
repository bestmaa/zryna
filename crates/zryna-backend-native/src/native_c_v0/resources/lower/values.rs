//! Dense source values evaluated once in original statement order, never reconstructed from text.

use super::super::{super::invariant_error, state::State};
use cranelift_codegen::ir::{InstBuilder, types};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::contract::{FlowStep, Primitive, ValueKind};

pub(super) fn through(state: &mut State<'_, '_>, expression: usize) -> Result<(), Diagnostic> {
    if expression >= state.values.len() {
        return Err(invariant_error());
    }
    while state.cursor <= expression {
        let id = state.cursor;
        let definition = state.environment.function.values().get(id).ok_or_else(invariant_error)?;
        let ordinary = match &definition.kind {
            ValueKind::I32(value) => {
                Some(state.builder.ins().iconst(types::I32, i64::from(*value)))
            }
            ValueKind::Bool(value) => {
                Some(state.builder.ins().iconst(types::I32, i64::from(*value)))
            }
            ValueKind::Local(binding) => {
                Some(*state.locals.get(*binding).ok_or_else(invariant_error)?)
            }
            ValueKind::WrappingAdd(left, right) => {
                let left = state.value(*left)?;
                let right = state.value(*right)?;
                Some(state.builder.ins().iadd(left, right))
            }
            ValueKind::Key(_) => Some(state.builder.ins().iconst(types::I64, 0)),
            ValueKind::Primitive(Primitive::ByteLength, arguments) => {
                Some(super::super::storage::byte_length(
                    state,
                    *arguments.first().ok_or_else(invariant_error)?,
                )?)
            }
            ValueKind::Primitive(_, _) => None,
        };
        if let Some(value) = ordinary {
            state.set(id, value)?;
        }
        for effect in state.environment.function.effects() {
            let original = match effect.operation() {
                FlowStep::OutputSlot { expression, .. }
                | FlowStep::Reserve { expression, .. }
                | FlowStep::Call { expression, .. }
                | FlowStep::StatusGuard { expression, .. }
                | FlowStep::ReadOutput { expression, .. }
                | FlowStep::Take { expression, .. }
                | FlowStep::ConfirmRelease { expression, .. }
                | FlowStep::Return { expression, .. }
                | FlowStep::PrepareLoan { expression, .. }
                | FlowStep::Copy { expression, .. } => *expression,
            };
            if original == id {
                super::effects::apply(state, effect)?;
            }
        }
        if !state.terminated && state.values[id].is_none() {
            let zero = state.builder.ins().iconst(types::I32, 0);
            state.set(id, zero)?;
        }
        state.cursor += 1;
    }
    Ok(())
}
