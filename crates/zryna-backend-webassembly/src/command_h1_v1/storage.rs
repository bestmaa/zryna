use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, ExportKind, ExportSection, Function, FunctionSection,
    GlobalSection, GlobalType, Instruction, MemArg, MemorySection, MemoryType, Module, TypeSection,
    ValType,
};

pub(super) mod audit;
mod canonical;
mod language;
mod reallocation;
#[cfg(test)]
mod tests;

const LANGUAGE_START: i32 = 65_536;
const CANONICAL_START: i32 = 15_728_640;
const MEMORY_END: i32 = 16_777_216;
const LEDGER_START: i32 = 4096;
const LEDGER_END: i32 = LEDGER_START + 16 * 12;
const CURSOR: u32 = 6;
const LIVE: u32 = 7;
const ATTEMPTS: u32 = 8;
const FATAL: u32 = 9;
const COPY: u32 = 1;
const FIND: u32 = 5;
const ALLOCATE: u32 = 6;
const FREE: u32 = 7;

pub(super) fn encode() -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32], [ValType::I32]);
    types.ty().function([ValType::I32; 3], []);
    types.ty().function([ValType::I32; 4], [ValType::I32]);
    types.ty().function([ValType::I32; 3], [ValType::I32]);
    types.ty().function([], []);
    types.ty().function([ValType::I32; 2], [ValType::I32]);
    types.ty().function([ValType::I32], []);
    module.section(&types);
    let mut functions = FunctionSection::new();
    for ty in [0, 1, 2, 3, 4, 5, 5, 6, 0] {
        functions.function(ty);
    }
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 256,
        maximum: Some(256),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memories);
    let mut globals = GlobalSection::new();
    for value in [LANGUAGE_START, 0, 0, 0, 0, 0, CANONICAL_START, 0, 0, 0] {
        globals.global(
            GlobalType { val_type: ValType::I32, mutable: true, shared: false },
            &ConstExpr::i32_const(value),
        );
    }
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    for (index, name) in
        ["arena", "status", "drops", "live", "peak", "references"].into_iter().enumerate()
    {
        exports.export(name, ExportKind::Global, u32::try_from(index).expect("six globals"));
    }
    for (index, name) in [
        (0, "allocate"),
        (1, "copy"),
        (2, "realloc"),
        (3, "validate"),
        (4, "drain"),
        (8, "canonical-state"),
    ] {
        exports.export(name, ExportKind::Func, index);
    }
    module.section(&exports);
    let mut code = CodeSection::new();
    for body in [
        language::allocate(),
        language::copy(),
        reallocation::realloc(),
        reallocation::validate(),
        canonical::drain(),
        canonical::find(),
        canonical::allocate(),
        canonical::free(),
        state(),
    ] {
        code.function(&body);
    }
    module.section(&code);
    module.finish()
}

fn emit<'a>(body: &mut Function, instructions: impl IntoIterator<Item = Instruction<'a>>) {
    for instruction in instructions {
        body.instruction(&instruction);
    }
}

fn memory(offset: u64) -> MemArg {
    MemArg { offset, align: 2, memory_index: 0 }
}

fn fatal(body: &mut Function, reason: i32) {
    emit(
        body,
        [Instruction::I32Const(reason), Instruction::GlobalSet(FATAL), Instruction::Unreachable],
    );
}

fn require_live_store(body: &mut Function) {
    emit(body, [Instruction::GlobalGet(FATAL), Instruction::If(BlockType::Empty)]);
    body.instruction(&Instruction::Unreachable);
    body.instruction(&Instruction::End);
}

fn reject_if(body: &mut Function, reason: i32) {
    body.instruction(&Instruction::If(BlockType::Empty));
    fatal(body, reason);
    body.instruction(&Instruction::End);
}

fn state() -> Function {
    let mut body = Function::new([]);
    for (selector, global) in [(0, CURSOR), (1, LIVE), (2, ATTEMPTS), (3, FATAL)] {
        emit(
            &mut body,
            [
                Instruction::LocalGet(0),
                Instruction::I32Const(selector),
                Instruction::I32Eq,
                Instruction::If(BlockType::Empty),
                Instruction::GlobalGet(global),
                Instruction::Return,
                Instruction::End,
            ],
        );
    }
    emit(&mut body, [Instruction::I32Const(-1), Instruction::End]);
    body
}
