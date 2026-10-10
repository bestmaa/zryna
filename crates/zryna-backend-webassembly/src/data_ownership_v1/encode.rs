use wasm_encoder::{Function, Instruction, ValType};
use zryna_ir::data_ownership_v1::{FunctionIdentity, VerifiedFunction};

use super::error;

mod assembly;
mod cleanup;
mod clone_frontier;
mod command_environment;
mod control;
mod failure;
#[cfg(test)]
mod legacy_frontier_tests;
mod memory;
mod observation;
mod operations;
mod places;
mod values;

const MEMORY_PAGES: u64 = 256;

pub(super) fn module(
    program: &zryna_ir::data_ownership_v1::VerifiedProgram,
) -> Result<Vec<u8>, zryna_diagnostics::Diagnostic> {
    assembly::module(program)
}

pub(crate) fn command(
    program: &zryna_ir::command_h1_v1::VerifiedProgram,
) -> Result<Vec<u8>, zryna_diagnostics::Diagnostic> {
    assembly::command(program)
}

fn allocator() -> Function {
    let mut function = Function::new([(2, ValType::I32)]);
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::I32Const(
        i32::try_from(zryna_ownership_runtime_abi::MAX_DYNAMIC_ALLOCATION_BYTES)
            .expect("universal allocation byte limit fits i32"),
    ));
    function.instruction(&Instruction::I32GtU);
    function.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
    failure::helper_trap(3, &mut function);
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::GlobalGet(0));
    function.instruction(&Instruction::LocalTee(1));
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::I32Add);
    function.instruction(&Instruction::I32Const(7));
    function.instruction(&Instruction::I32Add);
    function.instruction(&Instruction::I32Const(-8));
    function.instruction(&Instruction::I32And);
    function.instruction(&Instruction::LocalTee(2));
    function.instruction(&Instruction::LocalGet(1));
    function.instruction(&Instruction::I32LtU);
    function.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
    failure::helper_trap(3, &mut function);
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::LocalGet(2));
    function.instruction(&Instruction::I32Const(
        i32::try_from(MEMORY_PAGES * 65_536).unwrap_or(i32::MAX),
    ));
    function.instruction(&Instruction::I32GtU);
    function.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
    failure::helper_trap(2, &mut function);
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::LocalGet(2));
    function.instruction(&Instruction::GlobalSet(0));
    function.instruction(&Instruction::LocalGet(1));
    function.instruction(&Instruction::End);
    function
}

pub(super) struct Context<'a> {
    functions: &'a [VerifiedFunction<'a>],
    pub(super) layouts: &'a zryna_layout::VerifiedLayouts,
    type_count: u32,
    program_base: u32,
    observation: u32,
    helper_base: u32,
    command: Option<CommandContext<'a>>,
    clone_frontier: bool,
}

struct CommandContext<'a> {
    key: &'a str,
    environment: u32,
}

impl Context<'_> {
    pub(super) fn function_index(
        &self,
        id: FunctionIdentity,
    ) -> Result<u32, zryna_diagnostics::Diagnostic> {
        self.functions
            .iter()
            .position(|function| function.id() == id)
            .and_then(|index| u32::try_from(index).ok())
            .and_then(|index| self.program_base.checked_add(index))
            .ok_or_else(index_error)
    }

    pub(super) const fn clone_index(&self, ty: zryna_layout::TypeId) -> u32 {
        self.helper_base + ty.index()
    }

    pub(super) fn drop_index(&self, ty: zryna_layout::TypeId) -> u32 {
        self.helper_base + self.type_count + ty.index()
    }
}

fn export_wrapper(
    function: VerifiedFunction<'_>,
    target: u32,
    clone_frontier: bool,
) -> Result<Function, zryna_diagnostics::Diagnostic> {
    let parameters = function.parameters().len() + function.borrow_parameters().len();
    let result = u32::try_from(parameters).map_err(|_| index_error())?;
    let mut body = Function::new([(1, ValType::I32)]);
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::GlobalSet(1));
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::GlobalSet(2));
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::GlobalSet(5));
    clone_frontier::clear(clone_frontier, &mut body);
    body.instruction(&Instruction::I32Const(observation::ARENA_START));
    body.instruction(&Instruction::GlobalSet(0));
    for index in 0..parameters {
        body.instruction(&Instruction::LocalGet(u32::try_from(index).map_err(|_| index_error())?));
    }
    body.instruction(&Instruction::Call(target));
    body.instruction(&Instruction::LocalSet(result));
    clone_frontier::clear(clone_frontier, &mut body);
    body.instruction(&Instruction::I32Const(observation::ARENA_START));
    body.instruction(&Instruction::GlobalSet(0));
    body.instruction(&Instruction::LocalGet(result));
    body.instruction(&Instruction::End);
    Ok(body)
}

fn copy_memory() -> Function {
    let mut body = Function::new([(1, ValType::I32)]);
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::LocalSet(3));
    body.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
    body.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
    body.instruction(&Instruction::LocalGet(3));
    body.instruction(&Instruction::LocalGet(2));
    body.instruction(&Instruction::I32GeU);
    body.instruction(&Instruction::BrIf(1));
    body.instruction(&Instruction::LocalGet(0));
    body.instruction(&Instruction::LocalGet(3));
    body.instruction(&Instruction::I32Add);
    body.instruction(&Instruction::LocalGet(1));
    body.instruction(&Instruction::LocalGet(3));
    body.instruction(&Instruction::I32Add);
    body.instruction(&Instruction::I32Load8U(wasm_encoder::MemArg {
        offset: 0,
        align: 0,
        memory_index: 0,
    }));
    body.instruction(&Instruction::I32Store8(wasm_encoder::MemArg {
        offset: 0,
        align: 0,
        memory_index: 0,
    }));
    body.instruction(&Instruction::LocalGet(3));
    body.instruction(&Instruction::I32Const(1));
    body.instruction(&Instruction::I32Add);
    body.instruction(&Instruction::LocalSet(3));
    body.instruction(&Instruction::Br(0));
    body.instruction(&Instruction::End);
    body.instruction(&Instruction::End);
    body.instruction(&Instruction::End);
    body
}

#[derive(Clone, Copy)]
pub(super) struct Locals {
    pub(super) frame: u32,
    pub(super) borrows: u32,
    pub(super) scratch: u32,
    pub(super) heap: u32,
    pub(super) state: u32,
}

#[allow(clippy::too_many_lines)]
fn encode_function(
    function: VerifiedFunction<'_>,
    context: &Context<'_>,
) -> Result<Function, zryna_diagnostics::Diagnostic> {
    let parameter_count = function.parameters().len() + function.borrow_parameters().len();
    let value_count = function
        .blocks()
        .flat_map(zryna_ir::data_ownership_v1::VerifiedBlock::instructions)
        .filter_map(zryna_ir::data_ownership_v1::VerifiedInstruction::result)
        .map(|id| id.index() + 1)
        .chain(function.parameters().map(|value| value.id().index() + 1))
        .chain(
            function
                .blocks()
                .flat_map(zryna_ir::data_ownership_v1::VerifiedBlock::parameters)
                .map(|value| value.id().index() + 1),
        )
        .max()
        .unwrap_or(0);
    let borrow_count = function
        .borrow_parameters()
        .map(|borrow| borrow.id().index() + 1)
        .chain(
            function
                .blocks()
                .flat_map(zryna_ir::data_ownership_v1::VerifiedBlock::instructions)
                .filter_map(zryna_ir::data_ownership_v1::VerifiedInstruction::borrow)
                .map(|id| id.index() + 1),
        )
        .max()
        .unwrap_or(0);
    let max_edge = function
        .blocks()
        .map(|block| {
            block.terminator().edges().map(|edge| edge.arguments().len()).max().unwrap_or(0) + 1
        })
        .max()
        .unwrap_or(1);
    let local_base = value_count.max(u32::try_from(parameter_count).map_err(|_| index_error())?);
    let locals = Locals {
        frame: local_base,
        borrows: local_base + 1,
        scratch: local_base + 1 + borrow_count,
        heap: local_base + 1 + borrow_count + u32::try_from(max_edge).map_err(|_| index_error())?,
        state: local_base
            + 5
            + borrow_count
            + u32::try_from(max_edge).map_err(|_| index_error())?,
    };
    let declared = locals.state + 1 - u32::try_from(parameter_count).map_err(|_| index_error())?;
    let mut body = Function::new((declared > 0).then_some((declared, ValType::I32)));
    let frame_bytes = function
        .places()
        .len()
        .checked_mul(4)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(index_error)?;
    body.instruction(&Instruction::I32Const(frame_bytes));
    body.instruction(&Instruction::Call(0));
    body.instruction(&Instruction::LocalSet(locals.frame));
    body.instruction(&Instruction::GlobalGet(1));
    body.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
    for parameter in function.parameters().collect::<Vec<_>>().into_iter().rev() {
        if context.layouts.type_by_id(parameter.ty()).ok_or_else(index_error)?.drop_kind() != 0 {
            body.instruction(&Instruction::LocalGet(parameter.id().index()));
            body.instruction(&Instruction::Call(context.drop_index(parameter.ty())));
        }
    }
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::Return);
    body.instruction(&Instruction::End);
    for place in function.places() {
        if let zryna_ir::data_ownership_v1::VerifiedPlaceKind::Parameter(ordinal) = place.kind() {
            operations::place_address(
                function,
                place.id().index(),
                locals,
                context.layouts,
                &mut body,
            )?;
            body.instruction(&Instruction::LocalGet(ordinal));
            operations::store(&mut body);
        }
    }
    for (ordinal, borrow) in function.borrow_parameters().enumerate() {
        body.instruction(&Instruction::LocalGet(
            u32::try_from(function.parameters().len() + ordinal).map_err(|_| index_error())?,
        ));
        body.instruction(&Instruction::LocalSet(locals.borrows + borrow.id().index()));
    }
    body.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
    let block_parameters =
        function.blocks().map(|block| block.parameters().collect::<Vec<_>>()).collect::<Vec<_>>();
    for block in function.blocks() {
        body.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        body.instruction(&Instruction::LocalGet(locals.state));
        body.instruction(&Instruction::I32Const(
            i32::try_from(block.id().index()).map_err(|_| index_error())?,
        ));
        body.instruction(&Instruction::I32Ne);
        body.instruction(&Instruction::BrIf(0));
        for instruction in block.instructions() {
            body.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
            operations::instruction(function, instruction, locals, context, &mut body)?;
            body.instruction(&Instruction::End);
            failure::propagate(function, instruction, locals, context, &mut body)?;
        }
        control::terminator(
            function,
            block.terminator(),
            &block_parameters,
            locals,
            context,
            &mut body,
        )?;
        body.instruction(&Instruction::End);
    }
    body.instruction(&Instruction::Unreachable);
    body.instruction(&Instruction::End);
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::End);
    Ok(body)
}

pub(super) fn index_error() -> zryna_diagnostics::Diagnostic {
    error("ZRYNA-W3001", "verified DataOwnershipV1 index exceeds core WebAssembly limits")
}

#[cfg(test)]
mod allocator_tests;
