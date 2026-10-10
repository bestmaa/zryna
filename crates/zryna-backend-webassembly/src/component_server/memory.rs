//! One fixed page; canonical allocations are bounded fresh allocations and never grow memory.

use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, ExportKind, ExportSection, Function, FunctionSection,
    GlobalSection, GlobalType, Instruction, MemorySection, MemoryType, Module, TypeSection,
    ValType,
};

pub(super) fn encode() -> Vec<u8> {
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32; 4], [ValType::I32]);
    let mut functions = FunctionSection::new();
    functions.function(0);
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 1,
        maximum: Some(1),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: true, shared: false },
        &ConstExpr::i32_const(1024),
    );
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("realloc", ExportKind::Func, 0);
    let mut realloc = Function::new([(1, ValType::I32)]);
    // Empty allocations return zero. Existing allocations/resizes are unsupported and trap;
    // the fixed arrangement only needs fresh allocation for an optional host error string.
    realloc.instruction(&Instruction::LocalGet(3));
    realloc.instruction(&Instruction::I32Eqz);
    realloc.instruction(&Instruction::If(BlockType::Empty));
    realloc.instruction(&Instruction::I32Const(0));
    realloc.instruction(&Instruction::Return);
    realloc.instruction(&Instruction::End);
    realloc.instruction(&Instruction::LocalGet(0));
    realloc.instruction(&Instruction::LocalGet(1));
    realloc.instruction(&Instruction::I32Or);
    trap_if(&mut realloc);
    // Canonical alignments are powers of two up to eight for this WIT graph.
    realloc.instruction(&Instruction::LocalGet(2));
    realloc.instruction(&Instruction::I32Eqz);
    trap_if(&mut realloc);
    realloc.instruction(&Instruction::LocalGet(2));
    realloc.instruction(&Instruction::I32Const(8));
    realloc.instruction(&Instruction::I32GtU);
    trap_if(&mut realloc);
    realloc.instruction(&Instruction::LocalGet(2));
    realloc.instruction(&Instruction::LocalGet(2));
    realloc.instruction(&Instruction::I32Const(1));
    realloc.instruction(&Instruction::I32Sub);
    realloc.instruction(&Instruction::I32And);
    trap_if(&mut realloc);
    realloc.instruction(&Instruction::GlobalGet(0));
    realloc.instruction(&Instruction::LocalGet(2));
    realloc.instruction(&Instruction::I32Const(1));
    realloc.instruction(&Instruction::I32Sub);
    realloc.instruction(&Instruction::I32Add);
    realloc.instruction(&Instruction::I32Const(0));
    realloc.instruction(&Instruction::LocalGet(2));
    realloc.instruction(&Instruction::I32Sub);
    realloc.instruction(&Instruction::I32And);
    realloc.instruction(&Instruction::LocalTee(4));
    realloc.instruction(&Instruction::I32Const(65_536));
    realloc.instruction(&Instruction::I32GtU);
    trap_if(&mut realloc);
    realloc.instruction(&Instruction::LocalGet(3));
    realloc.instruction(&Instruction::I32Const(65_536));
    realloc.instruction(&Instruction::LocalGet(4));
    realloc.instruction(&Instruction::I32Sub);
    realloc.instruction(&Instruction::I32GtU);
    trap_if(&mut realloc);
    realloc.instruction(&Instruction::LocalGet(4));
    realloc.instruction(&Instruction::LocalGet(3));
    realloc.instruction(&Instruction::I32Add);
    realloc.instruction(&Instruction::GlobalSet(0));
    realloc.instruction(&Instruction::LocalGet(4));
    realloc.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&realloc);
    let mut module = Module::new();
    module.section(&types);
    module.section(&functions);
    module.section(&memories);
    module.section(&globals);
    module.section(&exports);
    module.section(&code);
    module.finish()
}

pub(super) fn trap_if(function: &mut Function) {
    function.instruction(&Instruction::If(BlockType::Empty));
    function.instruction(&Instruction::Unreachable);
    function.instruction(&Instruction::End);
}
