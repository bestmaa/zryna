//! Whole-value transfer and actual fixed runtime calls, with sealed failure cleanup.
use super::{
    body::{get, store},
    codegen, invariant,
};
use cranelift_codegen::ir::{
    FuncRef, InstBuilder, StackSlot, StackSlotData, StackSlotKind, TrapCode, Value,
    condcodes::IntCC, types,
};
use cranelift_frontend::FunctionBuilder;
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::{
    owned_v2::raw::{Operation, Step},
    raw,
};
use zryna_native_mir::generic_owned_v2::VerifiedProgram;

#[derive(Clone, Copy)]
pub(super) struct Context<'c, 'p, 'a> {
    pub p: &'c VerifiedProgram<'p, 'a>,
    pub record: StackSlot,
    pub runtime: &'c [FuncRef],
    pub f: &'c raw::Function,
}
pub(super) fn operation(
    context: &Context<'_, '_, '_>,
    b: &mut FunctionBuilder<'_>,
    values: &[Option<Vec<Value>>],
    op: &Operation,
    step: Option<&Step>,
) -> Result<Vec<Value>, Diagnostic> {
    let Context { record, runtime, f, .. } = *context;
    Ok(match op {
        Operation::Move(id) | Operation::Borrow { value: id, .. } => get(values, *id)?.to_vec(),
        Operation::EndLoan(_) => Vec::new(),
        Operation::Drop(id) => {
            drop_value(context, b, get(values, *id)?, value_type(f, *id)?)?;
            Vec::new()
        }
        Operation::StringLiteral(_) | Operation::CloneString(_) => {
            let out = b.ins().stack_addr(types::I64, record, 0);
            let call = match op {
                Operation::StringLiteral(bytes) => {
                    let slot = b.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        u32::try_from(bytes.len().max(1)).map_err(codegen)?,
                        0,
                    ));
                    for (j, byte) in bytes.iter().enumerate() {
                        let value = b.ins().iconst(types::I8, i64::from(*byte));
                        b.ins().stack_store(
                            types::I64,
                            value,
                            slot,
                            i32::try_from(j).map_err(codegen)?,
                        );
                    }
                    let pointer = b.ins().stack_addr(types::I64, slot, 0);
                    let length =
                        b.ins().iconst(types::I64, i64::try_from(bytes.len()).map_err(codegen)?);
                    b.ins().call(runtime[0], &[pointer, length, out])
                }
                Operation::CloneString(id) => {
                    let pointer = b.ins().stack_addr(types::I64, record, 24);
                    store(b, pointer, get(values, *id)?)?;
                    b.ins().call(runtime[1], &[pointer, out])
                }
                _ => return Err(invariant()),
            };
            let status = b.inst_results(call)[0];
            failure(context, b, values, step.ok_or_else(invariant)?, status)?;
            (0..3)
                .map(|lane| b.ins().stack_load(types::I64, types::I64, record, lane * 8))
                .collect()
        }
    })
}
fn value_type(f: &raw::Function, id: u32) -> Result<raw::Type, Diagnostic> {
    f.blocks
        .iter()
        .flat_map(|b| b.parameters.iter().chain(b.instructions.iter().map(|i| &i.result)))
        .find(|d| d.id == id)
        .map(|d| d.ty)
        .ok_or_else(invariant)
}
pub(super) fn cleanup(
    context: &Context<'_, '_, '_>,
    b: &mut FunctionBuilder<'_>,
    values: &[Option<Vec<Value>>],
    step: &Step,
) -> Result<(), Diagnostic> {
    let f = context.f;
    for id in &step.cleanup {
        drop_value(context, b, get(values, *id)?, value_type(f, *id)?)?;
    }
    Ok(())
}
pub(super) fn failure(
    context: &Context<'_, '_, '_>,
    b: &mut FunctionBuilder<'_>,
    values: &[Option<Vec<Value>>],
    step: &Step,
    status: Value,
) -> Result<(), Diagnostic> {
    let failed = b.create_block();
    let success = b.create_block();
    b.ins().brif(status, failed, &[], success, &[]);
    b.switch_to_block(failed);
    cleanup(context, b, values, step)?;
    b.ins().return_(&[status]);
    b.switch_to_block(success);
    Ok(())
}
fn drop_value(
    context: &Context<'_, '_, '_>,
    b: &mut FunctionBuilder<'_>,
    lanes: &[Value],
    ty: raw::Type,
) -> Result<(), Diagnostic> {
    let Context { p, record, runtime, .. } = *context;
    let raw::Type::Stored(index) = ty else {
        return Err(invariant());
    };
    let view = p.stored_type(index).ok_or_else(invariant)?;
    if view.drop_kind() == 0 {
        return Ok(());
    }
    match view.key()[0] {
        2 => {
            let pointer = b.ins().stack_addr(types::I64, record, 24);
            store(b, pointer, lanes)?;
            let call = b.ins().call(runtime[2], &[pointer]);
            let status = b.inst_results(call)[0];
            b.ins().trapnz(status, TrapCode::unwrap_user(7));
        }
        0x14 | 0x15 => {
            let done = b.create_block();
            for (ordinal, (_, payload)) in view.variants().enumerate() {
                let active = b.create_block();
                let next = b.create_block();
                let test = b.ins().icmp_imm_u(
                    IntCC::Equal,
                    lanes[0],
                    i64::try_from(ordinal).map_err(codegen)?,
                );
                b.ins().brif(test, active, &[], next, &[]);
                b.switch_to_block(active);
                if let Some(payload) = payload {
                    let child = raw::Type::Stored(payload.index());
                    let width = p.width(child)? as usize;
                    drop_value(context, b, lanes.get(1..1 + width).ok_or_else(invariant)?, child)?;
                }
                b.ins().jump(done, &[]);
                b.switch_to_block(next);
            }
            b.ins().trap(TrapCode::unwrap_user(7));
            b.switch_to_block(done);
        }
        _ => return Err(invariant()),
    }
    Ok(())
}
