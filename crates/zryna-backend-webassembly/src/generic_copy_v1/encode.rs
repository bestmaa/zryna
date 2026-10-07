//! Direct private-lane core encoding. Only the separately sealed Copy authority is read.

use super::{
    bytes::Bytes,
    control, invariant,
    layout::{Layout, Locals, Value},
};
use wasm_encoder::{BlockType, Instruction as Op};
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::raw;

pub(super) fn module(layout: &Layout<'_, '_>) -> Result<Vec<u8>, Diagnostic> {
    let mut module = Bytes::new();
    module.extend(b"\0asm\x01\0\0\0")?;
    let count = layout.functions.len() + layout.program.export_functions().len();
    let mut types = Bytes::new();
    types.count(count)?;
    for locals in &layout.functions {
        signature(&mut types, locals.parameters, false)?;
    }
    for export in layout.program.scalar_abi().exports() {
        signature(
            &mut types,
            u32::try_from(export.parameters().len()).map_err(|_| super::budget())?,
            true,
        )?;
    }
    module.section(1, &types)?;
    let mut functions = Bytes::new();
    functions.count(count)?;
    for index in 0..count {
        functions.count(index)?;
    }
    module.section(3, &functions)?;
    let mut globals = Bytes::new();
    globals.u32(layout.return_lanes)?;
    for _ in 0..layout.return_lanes {
        globals.extend(&[0x7f, 0x01])?;
        globals.op(&Op::I32Const(0))?;
        globals.op(&Op::End)?;
    }
    module.section(6, &globals)?;
    let mut exports = Bytes::new();
    exports.count(layout.program.export_functions().len())?;
    for (index, export) in layout.program.scalar_abi().exports().enumerate() {
        exports.name(export.webassembly_name().as_str())?;
        exports.extend(&[0])?;
        exports.count(layout.functions.len() + index)?;
    }
    module.section(7, &exports)?;
    let mut code = Bytes::new();
    code.count(count)?;
    for (index, function) in layout.program.functions().iter().enumerate() {
        let body = function_body(layout, function, &layout.functions[index])?;
        code.count(body.bytes.len())?;
        code.extend(&body.bytes)?;
    }
    for function in layout.program.export_functions() {
        let body = wrapper(layout, *function)?;
        code.count(body.bytes.len())?;
        code.extend(&body.bytes)?;
    }
    module.section(10, &code)?;
    Ok(module.bytes)
}

fn signature(bytes: &mut Bytes, parameters: u32, result: bool) -> Result<(), Diagnostic> {
    bytes.extend(&[0x60])?;
    bytes.u32(parameters)?;
    for _ in 0..parameters {
        bytes.extend(&[0x7f])?;
    }
    bytes.extend(if result { &[1, 0x7f] } else { &[0] })
}

fn function_body(
    layout: &Layout<'_, '_>,
    function: &raw::Function,
    locals: &Locals,
) -> Result<Bytes, Diagnostic> {
    let mut body = Bytes::new();
    body.extend(&[1])?;
    body.u32(locals.declared)?;
    body.extend(&[0x7f])?;
    let mut argument = 0;
    for parameter in &function.blocks[0].parameters {
        let value = locals.values[parameter.id as usize];
        for lane in 0..value.width {
            body.op(&Op::LocalGet(argument))?;
            body.op(&Op::LocalSet(value.start + lane))?;
            argument += 1;
        }
    }
    if argument != locals.parameters {
        return Err(invariant());
    }
    body.op(&Op::Loop(BlockType::Empty))?;
    for block in &function.blocks {
        body.op(&Op::Block(BlockType::Empty))?;
        body.op(&Op::LocalGet(locals.state))?;
        body.op(&Op::I32Const(i32::try_from(block.id).map_err(|_| super::budget())?))?;
        body.op(&Op::I32Ne)?;
        body.op(&Op::BrIf(0))?;
        for instruction in &block.instructions {
            operation(&mut body, layout, locals, instruction)?;
        }
        control::terminator(&mut body, function, locals, &block.terminator)?;
        body.op(&Op::End)?;
    }
    body.op(&Op::Unreachable)?;
    body.op(&Op::End)?;
    body.op(&Op::End)?;
    Ok(body)
}

fn operation(
    body: &mut Bytes,
    layout: &Layout<'_, '_>,
    locals: &Locals,
    instruction: &raw::Instruction,
) -> Result<(), Diagnostic> {
    let result = locals.values[instruction.result.id as usize];
    let value = |id: u32| locals.values[id as usize];
    match &instruction.operation {
        raw::Operation::BoolLiteral(boolean) => {
            scalar(body, result, &Op::I32Const(i32::from(*boolean)))
        }
        raw::Operation::I32Literal(integer) => scalar(body, result, &Op::I32Const(*integer)),
        raw::Operation::Unit if result.width == 0 => Ok(()),
        raw::Operation::Unit => Err(invariant()),
        raw::Operation::Copy { value: id } => {
            let source = value(*id);
            if source.width != result.width {
                return Err(invariant());
            }
            for lane in 0..result.width {
                body.op(&Op::LocalGet(source.start + lane))?;
                body.op(&Op::LocalSet(result.start + lane))?;
            }
            Ok(())
        }
        raw::Operation::I32Add { left, right } => {
            body.op(&Op::LocalGet(value(*left).start))?;
            body.op(&Op::LocalGet(value(*right).start))?;
            scalar(body, result, &Op::I32Add)
        }
        raw::Operation::ClosedGenericCall { instance, arguments } => {
            call(body, layout, locals, *instance, arguments, result)
        }
        raw::Operation::SourceCall { module, function, arguments } => {
            call(body, layout, locals, layout.source_call(*module, *function)?, arguments, result)
        }
        raw::Operation::ClosedEnumConstruct { ordinal, payload, .. } => {
            if result.width == 0 {
                return Err(invariant());
            }
            body.op(&Op::I32Const(i32::try_from(*ordinal).map_err(|_| invariant())?))?;
            body.op(&Op::LocalSet(result.start))?;
            let payload = payload.map(value);
            for lane in 1..result.width {
                body.op(&if let Some(payload) = payload.filter(|payload| lane <= payload.width) {
                    Op::LocalGet(payload.start + lane - 1)
                } else {
                    Op::I32Const(0)
                })?;
                body.op(&Op::LocalSet(result.start + lane))?;
            }
            Ok(())
        }
    }
}

fn scalar(body: &mut Bytes, result: Value, operation: &Op<'_>) -> Result<(), Diagnostic> {
    if result.width != 1 {
        return Err(invariant());
    }
    body.op(operation)?;
    body.op(&Op::LocalSet(result.start))
}

fn call(
    body: &mut Bytes,
    layout: &Layout<'_, '_>,
    locals: &Locals,
    function: u32,
    arguments: &[u32],
    result: Value,
) -> Result<(), Diagnostic> {
    let callee = layout.program.functions().get(function as usize).ok_or_else(invariant)?;
    if layout.width(callee.result)? != result.width {
        return Err(invariant());
    }
    for argument in arguments {
        let value = locals.values[*argument as usize];
        for lane in 0..value.width {
            body.op(&Op::LocalGet(value.start + lane))?;
        }
    }
    body.op(&Op::Call(function))?;
    for lane in 0..result.width {
        body.op(&Op::GlobalGet(lane))?;
        body.op(&Op::LocalSet(result.start + lane))?;
    }
    Ok(())
}

fn wrapper(layout: &Layout<'_, '_>, index: usize) -> Result<Bytes, Diagnostic> {
    let function = &layout.program.functions()[index];
    let mut body = Bytes::new();
    body.extend(&[0])?;
    for (index, parameter) in function.parameters.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| super::budget())?;
        if *parameter == raw::Type::Stored(0) {
            bool_guard(&mut body, &Op::LocalGet(index))?;
        }
        body.op(&Op::LocalGet(index))?;
    }
    body.op(&Op::Call(u32::try_from(index).map_err(|_| super::budget())?))?;
    if function.result == raw::Type::Stored(0) {
        bool_guard(&mut body, &Op::GlobalGet(0))?;
    }
    body.op(&Op::GlobalGet(0))?;
    body.op(&Op::End)?;
    Ok(body)
}

fn bool_guard(body: &mut Bytes, carrier: &Op<'_>) -> Result<(), Diagnostic> {
    body.op(carrier)?;
    body.op(&Op::I32Const(0))?;
    body.op(&Op::I32Eq)?;
    body.op(carrier)?;
    body.op(&Op::I32Const(1))?;
    body.op(&Op::I32Eq)?;
    body.op(&Op::I32Or)?;
    body.op(&Op::If(BlockType::Empty))?;
    body.op(&Op::Else)?;
    body.op(&Op::Unreachable)?;
    body.op(&Op::End)
}
