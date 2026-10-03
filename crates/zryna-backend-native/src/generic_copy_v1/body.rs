//! Direct calls and immutable lane definitions; result pointers never escape a private call.

use super::{codegen, control, invariant};
use cranelift_codegen::{
    Context,
    ir::{FuncRef, InstBuilder, MemFlagsData, StackSlotData, StackSlotKind, Value, types},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use std::collections::{BTreeMap, BTreeSet};
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::raw;
use zryna_native_mir::generic_copy_v1::VerifiedProgram;

pub(super) fn target(
    program: &VerifiedProgram<'_, '_>,
    op: &raw::Operation,
) -> Result<Option<usize>, Diagnostic> {
    match op {
        raw::Operation::ClosedGenericCall { instance, .. } => Ok(Some(*instance as usize)),
        raw::Operation::SourceCall { module, function, .. } => {
            program.source_call(*module, *function).map(Some)
        }
        _ => Ok(None),
    }
}

pub(super) fn callees(
    program: &VerifiedProgram<'_, '_>,
    index: usize,
) -> Result<BTreeSet<usize>, Diagnostic> {
    let f = &program.program().functions()[index];
    let mut result = BTreeSet::new();
    for block in program.functions()[index].block_order() {
        for i in &f.blocks[*block].instructions {
            if let Some(t) = target(program, &i.operation)? {
                result.insert(t);
            }
        }
    }
    Ok(result)
}

pub(super) fn build(
    program: &VerifiedProgram<'_, '_>,
    index: usize,
    context: &mut Context,
    bc: &mut FunctionBuilderContext,
    callees: &BTreeMap<usize, FuncRef>,
    config: cranelift_codegen::isa::TargetFrontendConfig,
) -> Result<(), Diagnostic> {
    let f = &program.program().functions()[index];
    let mut b = FunctionBuilder::new(&mut context.func, bc);
    let entry = b.create_block();
    b.append_block_params_for_function_params(entry);
    let parameters = b.block_params(entry).to_vec();
    let mut blocks = Vec::new();
    let count =
        f.blocks.iter().map(|block| block.parameters.len() + block.instructions.len()).sum();
    let mut values = vec![None; count];
    for block in &f.blocks {
        let encoded = b.create_block();
        for def in &block.parameters {
            let mut lanes = Vec::new();
            for _ in 0..program.width(def.ty)? {
                lanes.push(b.append_block_param(encoded, types::I32));
            }
            values[def.id as usize] = Some(lanes);
        }
        blocks.push(encoded);
    }
    b.switch_to_block(entry);
    b.ins().jump(blocks[0], &parameters[1..].iter().copied().map(Into::into).collect::<Vec<_>>());
    let max_result = callees.keys().map(|i| program.functions()[*i].result()).max().unwrap_or(0);
    let slot = b.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        max_result.max(1) * 4,
        2,
    ));
    for block in program.functions()[index].block_order() {
        b.switch_to_block(blocks[*block]);
        for i in &f.blocks[*block].instructions {
            let lanes = operation(program, &mut b, &values, i, slot, callees)?;
            if lanes.len() != program.width(i.result.ty)? as usize
                || values[i.result.id as usize].replace(lanes).is_some()
            {
                return Err(invariant());
            }
        }
        control::terminator(&mut b, &values, &blocks, &f.blocks[*block].terminator, parameters[0])?;
    }
    b.seal_all_blocks();
    b.finalize(config);
    Ok(())
}

fn operation(
    program: &VerifiedProgram<'_, '_>,
    b: &mut FunctionBuilder<'_>,
    values: &[Option<Vec<Value>>],
    i: &raw::Instruction,
    slot: cranelift_codegen::ir::StackSlot,
    callees: &BTreeMap<usize, FuncRef>,
) -> Result<Vec<Value>, Diagnostic> {
    Ok(match &i.operation {
        raw::Operation::BoolLiteral(value) => vec![b.ins().iconst(types::I32, i64::from(*value))],
        raw::Operation::I32Literal(value) => vec![b.ins().iconst(types::I32, i64::from(*value))],
        raw::Operation::Unit => Vec::new(),
        raw::Operation::Copy { value } => get(values, *value)?.to_vec(),
        raw::Operation::I32Add { left, right } => {
            vec![b.ins().iadd(get(values, *left)?[0], get(values, *right)?[0])]
        }
        raw::Operation::ClosedEnumConstruct { ordinal, payload, .. } => {
            let mut lanes = vec![b.ins().iconst(types::I32, i64::from(*ordinal))];
            if let Some(payload) = payload {
                lanes.extend_from_slice(get(values, *payload)?);
            }
            while lanes.len() < program.width(i.result.ty)? as usize {
                lanes.push(b.ins().iconst(types::I32, 0));
            }
            lanes
        }
        raw::Operation::ClosedGenericCall { arguments, .. }
        | raw::Operation::SourceCall { arguments, .. } => {
            let target = target(program, &i.operation)?.ok_or_else(invariant)?;
            let mut args = vec![b.ins().stack_addr(types::I64, slot, 0)];
            for id in arguments {
                args.extend_from_slice(get(values, *id)?);
            }
            b.ins().call(*callees.get(&target).ok_or_else(invariant)?, &args);
            let mut lanes = Vec::new();
            for lane in 0..program.functions()[target].result() {
                lanes.push(b.ins().stack_load(
                    types::I64,
                    types::I32,
                    slot,
                    i32::try_from(lane * 4).map_err(codegen)?,
                ));
            }
            lanes
        }
    })
}

pub(super) fn get(values: &[Option<Vec<Value>>], id: u32) -> Result<&[Value], Diagnostic> {
    values.get(id as usize).and_then(Option::as_deref).ok_or_else(invariant)
}

pub(super) fn store(
    b: &mut FunctionBuilder<'_>,
    pointer: Value,
    lanes: &[Value],
) -> Result<(), Diagnostic> {
    for (i, lane) in lanes.iter().enumerate() {
        b.ins().store(
            MemFlagsData::trusted(),
            *lane,
            pointer,
            i32::try_from(i * 4).map_err(codegen)?,
        );
    }
    Ok(())
}
