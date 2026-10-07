//! Real String runtime calls and complete sealed cleanup before status propagation.
use super::{
    bytes::Bytes,
    invariant,
    layout::{Layout, Locals, Value},
    runtime,
};
use wasm_encoder::{BlockType, Instruction as Op};
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::{
    owned_v2::raw::{Operation, Step},
    raw,
};
use zryna_layout::{StorageTarget, TypeCategory};
pub(super) fn operation(
    body: &mut Bytes,
    layout: &Layout<'_, '_>,
    fi: usize,
    f: &raw::Function,
    locals: &Locals,
    i: &raw::Instruction,
    op: &Operation,
) -> Result<(), Diagnostic> {
    let result = locals.values[i.result.id as usize];
    match op {
        Operation::Move(id) | Operation::Borrow { value: id, .. } => {
            let source = locals.values[*id as usize];
            if source.width != result.width {
                return Err(invariant());
            }
            for lane in 0..result.width {
                body.op(&Op::LocalGet(source.start + lane))?;
                body.op(&Op::LocalSet(result.start + lane))?;
            }
        }
        Operation::EndLoan(_) => {}
        Operation::Drop(id) => {
            drop_value(body, layout, locals.values[*id as usize], value_type(f, *id)?)?;
        }
        Operation::StringLiteral(_)
        | Operation::CloneString(_)
        | Operation::CloneBorrowedString(_) => {
            match op {
                Operation::StringLiteral(_) => {
                    let (offset, bytes) =
                        layout.literals.get(&(fi, i.result.id)).ok_or_else(invariant)?;
                    body.op(&Op::I32Const(i32::try_from(*offset).map_err(|_| super::budget())?))?;
                    body.op(&Op::I32Const(
                        i32::try_from(bytes.len()).map_err(|_| super::budget())?,
                    ))?;
                    runtime::record(body, locals.frame, 0)?;
                    body.op(&Op::Call(0))?;
                }
                Operation::CloneString(id) | Operation::CloneBorrowedString(id) => {
                    let source = locals.values[*id as usize];
                    for lane in 0..3 {
                        body.op(&Op::LocalGet(source.start + lane))?;
                    }
                    runtime::record(body, locals.frame, 0)?;
                    body.op(&Op::Call(1))?;
                }
                _ => return Err(invariant()),
            }
            body.op(&Op::LocalSet(locals.status))?;
            // Failure must not read an unwritten output record. The enclosing site then cleans.
            body.op(&Op::LocalGet(locals.status))?;
            body.op(&Op::I32Eqz)?;
            body.op(&Op::If(BlockType::Empty))?;
            for lane in 0..3 {
                runtime::load(body, locals.frame, lane)?;
                body.op(&Op::LocalSet(result.start + lane))?;
            }
            body.op(&Op::End)?;
        }
    }
    Ok(())
}
pub(super) fn value_type(f: &raw::Function, id: u32) -> Result<raw::Type, Diagnostic> {
    f.blocks
        .iter()
        .flat_map(|b| b.parameters.iter().chain(b.instructions.iter().map(|i| &i.result)))
        .find(|d| d.id == id)
        .map(|d| d.ty)
        .ok_or_else(invariant)
}
pub(super) fn cleanup(
    body: &mut Bytes,
    layout: &Layout<'_, '_>,
    f: &raw::Function,
    locals: &Locals,
    step: &Step,
) -> Result<(), Diagnostic> {
    for id in &step.cleanup {
        drop_value(body, layout, locals.values[*id as usize], value_type(f, *id)?)?;
    }
    Ok(())
}
pub(super) fn restore(
    body: &mut Bytes,
    layout: &Layout<'_, '_>,
    locals: &Locals,
) -> Result<(), Diagnostic> {
    body.op(&Op::LocalGet(locals.frame))?;
    body.op(&Op::GlobalSet(layout.return_lanes + 1))
}
pub(super) fn failure(
    body: &mut Bytes,
    layout: &Layout<'_, '_>,
    f: &raw::Function,
    locals: &Locals,
    step: &Step,
) -> Result<(), Diagnostic> {
    body.op(&Op::LocalGet(locals.status))?;
    body.op(&Op::If(BlockType::Empty))?;
    cleanup(body, layout, f, locals, step)?;
    restore(body, layout, locals)?;
    body.op(&Op::LocalGet(locals.status))?;
    body.op(&Op::Return)?;
    body.op(&Op::End)
}
fn drop_value(
    body: &mut Bytes,
    layout: &Layout<'_, '_>,
    value: Value,
    ty: raw::Type,
) -> Result<(), Diagnostic> {
    let raw::Type::Stored(index) = ty else {
        return Err(invariant());
    };
    let ty = layout
        .program
        .layouts(StorageTarget::Linear32V1)
        .types()
        .nth(index as usize)
        .ok_or_else(invariant)?;
    if ty.drop_kind() == 0 {
        return Ok(());
    }
    match ty.category() {
        TypeCategory::String => {
            for lane in 0..3 {
                body.op(&Op::LocalGet(value.start + lane))?;
            }
            body.op(&Op::Call(2))?;
            body.op(&Op::If(BlockType::Empty))?;
            body.op(&Op::Unreachable)?;
            body.op(&Op::End)?;
        }
        TypeCategory::Enum => {
            for (ordinal, (_, payload)) in ty.variants().enumerate() {
                body.op(&Op::LocalGet(value.start))?;
                body.op(&Op::I32Const(i32::try_from(ordinal).map_err(|_| super::budget())?))?;
                body.op(&Op::I32Eq)?;
                body.op(&Op::If(BlockType::Empty))?;
                if let Some(payload) = payload {
                    let child = raw::Type::Stored(payload.index());
                    drop_value(
                        body,
                        layout,
                        Value { start: value.start + 1, width: layout.width(child)? },
                        child,
                    )?;
                }
                body.op(&Op::End)?;
            }
        }
        _ => return Err(invariant()),
    }
    Ok(())
}
