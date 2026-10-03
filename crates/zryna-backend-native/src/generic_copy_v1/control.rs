//! Canonical selected-arm payload transfer and simultaneous SSA edge arguments.

use super::{
    body::{get, store},
    invariant,
};
use cranelift_codegen::ir::{Block, InstBuilder, TrapCode, Value, condcodes::IntCC};
use cranelift_frontend::FunctionBuilder;
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::raw;

pub(super) fn terminator(
    b: &mut FunctionBuilder<'_>,
    values: &[Option<Vec<Value>>],
    blocks: &[Block],
    t: &raw::Terminator,
    result_pointer: Value,
) -> Result<(), Diagnostic> {
    match t {
        raw::Terminator::Return(id) => {
            store(b, result_pointer, get(values, *id)?)?;
            b.ins().return_(&[]);
        }
        raw::Terminator::Jump(e) => {
            let args = arguments(values, e, &[])?;
            b.ins().jump(blocks[e.target as usize], &args);
        }
        raw::Terminator::Branch { condition, yes, no } => {
            let c = get(values, *condition)?[0];
            b.ins().brif(
                c,
                blocks[yes.target as usize],
                &arguments(values, yes, &[])?,
                blocks[no.target as usize],
                &arguments(values, no, &[])?,
            );
        }
        raw::Terminator::ClosedEnumMatch { scrutinee, arms, .. } => {
            let lanes = get(values, *scrutinee)?;
            for arm in arms {
                let next = b.create_block();
                let test = b.ins().icmp_imm_u(IntCC::Equal, lanes[0], i64::from(arm.ordinal));
                let active = if let Some(binding) = &arm.binding {
                    get(values, binding.id)?.len()
                } else {
                    0
                };
                let payload = lanes.get(1..1 + active).ok_or_else(invariant)?;
                b.ins().brif(
                    test,
                    blocks[arm.edge.target as usize],
                    &arguments(values, &arm.edge, payload)?,
                    next,
                    &[],
                );
                b.switch_to_block(next);
            }
            b.ins().trap(TrapCode::unwrap_user(7));
        }
    }
    Ok(())
}

fn arguments(
    values: &[Option<Vec<Value>>],
    edge: &raw::Edge,
    payload: &[Value],
) -> Result<Vec<cranelift_codegen::ir::BlockArg>, Diagnostic> {
    let mut result =
        payload.iter().copied().map(cranelift_codegen::ir::BlockArg::from).collect::<Vec<_>>();
    for id in &edge.arguments {
        result.extend(get(values, *id)?.iter().copied().map(cranelift_codegen::ir::BlockArg::from));
    }
    Ok(result)
}
