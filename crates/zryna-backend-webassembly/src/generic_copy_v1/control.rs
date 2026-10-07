//! Exact selected-arm control flow and parallel scalar-lane transfers.

use super::{
    bytes::Bytes,
    invariant,
    layout::{Locals, Value},
};
use wasm_encoder::{BlockType, Instruction as Op};
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::raw;

pub(super) fn terminator(
    body: &mut Bytes,
    function: &raw::Function,
    locals: &Locals,
    terminator: &raw::Terminator,
) -> Result<(), Diagnostic> {
    match terminator {
        raw::Terminator::Return(id) => {
            let value = locals.values[*id as usize];
            for lane in 0..value.width {
                body.op(&Op::LocalGet(value.start + lane))?;
                body.op(&Op::GlobalSet(lane))?;
            }
            body.op(&Op::Return)
        }
        raw::Terminator::Jump(jump) => {
            edge(body, function, locals, jump, None)?;
            body.op(&Op::Br(1))
        }
        raw::Terminator::Branch { condition, yes, no } => {
            body.op(&Op::LocalGet(locals.values[*condition as usize].start))?;
            body.op(&Op::If(BlockType::Empty))?;
            edge(body, function, locals, yes, None)?;
            body.op(&Op::Br(2))?;
            body.op(&Op::Else)?;
            edge(body, function, locals, no, None)?;
            body.op(&Op::Br(2))?;
            body.op(&Op::End)
        }
        raw::Terminator::ClosedEnumMatch {
            scrutinee, mode: raw::MatchMode::Value, arms, ..
        } => {
            let value = locals.values[*scrutinee as usize];
            for arm in arms {
                body.op(&Op::LocalGet(value.start))?;
                body.op(&Op::I32Const(i32::try_from(arm.ordinal).map_err(|_| invariant())?))?;
                body.op(&Op::I32Eq)?;
                body.op(&Op::If(BlockType::Empty))?;
                let payload = arm.binding.as_ref().map(|binding| Value {
                    start: value.start + 1,
                    width: locals.values[binding.id as usize].width,
                });
                edge(body, function, locals, &arm.edge, payload)?;
                body.op(&Op::Br(2))?;
                body.op(&Op::End)?;
            }
            body.op(&Op::Unreachable)
        }
        raw::Terminator::ClosedEnumMatch { .. } => Err(invariant()),
    }
}

fn edge(
    body: &mut Bytes,
    function: &raw::Function,
    locals: &Locals,
    edge: &raw::Edge,
    payload: Option<Value>,
) -> Result<(), Diagnostic> {
    let block = function.blocks.get(edge.target as usize).ok_or_else(invariant)?;
    let mut offset = 0;
    if let Some(payload) = payload {
        to_scratch(body, locals.scratch, payload)?;
        offset += payload.width;
    }
    for argument in &edge.arguments {
        let value = locals.values[*argument as usize];
        to_scratch(body, locals.scratch + offset, value)?;
        offset += value.width;
    }
    let mut transferred = 0;
    for parameter in &block.parameters {
        let value = locals.values[parameter.id as usize];
        for lane in 0..value.width {
            body.op(&Op::LocalGet(locals.scratch + transferred + lane))?;
            body.op(&Op::LocalSet(value.start + lane))?;
        }
        transferred += value.width;
    }
    if transferred != offset {
        return Err(invariant());
    }
    body.op(&Op::I32Const(i32::try_from(edge.target).map_err(|_| invariant())?))?;
    body.op(&Op::LocalSet(locals.state))
}

fn to_scratch(body: &mut Bytes, start: u32, value: Value) -> Result<(), Diagnostic> {
    for lane in 0..value.width {
        body.op(&Op::LocalGet(value.start + lane))?;
        body.op(&Op::LocalSet(start + lane))?;
    }
    Ok(())
}
