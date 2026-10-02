use super::Probe;
use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
    Instruction, MemorySection, MemoryType, Module, TypeSection, ValType,
};

pub(super) fn memory() -> Module {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32; 4], [ValType::I32]);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut memory = MemorySection::new();
    memory.memory(MemoryType {
        minimum: 1,
        maximum: Some(1),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memory);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("realloc", ExportKind::Func, 0);
    module.section(&exports);
    let mut realloc = Function::new([]);
    realloc.instruction(&Instruction::Unreachable);
    realloc.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&realloc);
    module.section(&code);
    module
}

pub(super) fn caller(probe: Probe) -> Module {
    let (params, result) = match probe {
        Probe::Environment | Probe::Filesystem | Probe::Process => (vec![ValType::I32], vec![]),
        Probe::Clock | Probe::Random => (vec![], vec![ValType::I64]),
        Probe::Network => (vec![], vec![ValType::I32]),
    };
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function(params, result);
    types.ty().function([], [ValType::I32]);
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import("host", "deny", EntityType::Function(0));
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(1);
    module.section(&functions);
    let mut exports = ExportSection::new();
    exports.export("run", ExportKind::Func, 1);
    module.section(&exports);
    let mut run = Function::new([]);
    match probe {
        Probe::Environment | Probe::Filesystem | Probe::Process => {
            run.instruction(&Instruction::I32Const(0));
            run.instruction(&Instruction::Call(0));
        }
        Probe::Clock | Probe::Random | Probe::Network => {
            run.instruction(&Instruction::Call(0));
            run.instruction(&Instruction::Drop);
        }
    }
    run.instruction(&Instruction::I32Const(0));
    run.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&run);
    module.section(&code);
    module
}
