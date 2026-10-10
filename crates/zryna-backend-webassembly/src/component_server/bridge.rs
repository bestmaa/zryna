//! The only admitted guest arrangement: verified status, one complete empty response.

use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
    Instruction, MemArg, MemoryType, Module, TypeSection, ValType,
};
use zryna_diagnostics::Diagnostic;

use super::{
    ServerOperation, invalid,
    memory::trap_if,
    world::{CLOCK, HTTP, OPERATIONS, World},
};

pub(super) const HOST: &str = "server-host";
pub(super) const SCALAR: &str = "scalar-core";
pub(super) const MEMORY: &str = "canonical-memory";
pub(super) const DROP_REQUEST: &str = "drop-incoming-request";

pub(super) fn encode(
    world: &World,
    export: &str,
    operation: ServerOperation,
) -> Result<Vec<u8>, Diagnostic> {
    let mut types = TypeSection::new();
    let mut imports = ImportSection::new();
    let expected = [
        (vec![], vec![ValType::I32]),
        (vec![ValType::I32], vec![ValType::I32]),
        (vec![ValType::I32; 2], vec![ValType::I32]),
        (vec![ValType::I32; 2], vec![]),
        (vec![ValType::I32; 4], vec![]),
        (
            vec![
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I64,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
            ],
            vec![],
        ),
    ];
    for (index, name) in OPERATIONS.iter().enumerate() {
        let signature = world.signature(HTTP, name)?;
        if signature != expected[index] {
            return Err(invalid("server canonical ABI differs from the fixed response bridge"));
        }
        types.ty().function(signature.0, signature.1);
        imports.import(
            HOST,
            name,
            EntityType::Function(
                u32::try_from(index).map_err(|_| invalid("server function index overflow"))?,
            ),
        );
    }
    types.ty().function([ValType::I32], []);
    imports.import(HOST, DROP_REQUEST, EntityType::Function(6));
    types.ty().function([], [ValType::I32]);
    imports.import(SCALAR, export, EntityType::Function(7));
    let handle_index = if operation == ServerOperation::ClockRead {
        let signature = world.signature(CLOCK, "now")?;
        if signature != (vec![], vec![ValType::I64]) {
            return Err(invalid("server monotonic-clock ABI differs"));
        }
        types.ty().function(signature.0, signature.1);
        imports.import(HOST, "clock-now", EntityType::Function(8));
        9
    } else {
        8
    };
    imports.import(
        MEMORY,
        "memory",
        EntityType::Memory(MemoryType {
            minimum: 1,
            maximum: Some(1),
            memory64: false,
            shared: false,
            page_size_log2: None,
        }),
    );
    types.ty().function([ValType::I32; 2], []);
    let mut functions = FunctionSection::new();
    functions.function(handle_index);
    let mut exports = ExportSection::new();
    exports.export("handle", ExportKind::Func, handle_index);
    let mut code = CodeSection::new();
    code.function(&response_handle(operation));
    let mut module = Module::new();
    module.section(&types);
    module.section(&imports);
    module.section(&functions);
    module.section(&exports);
    module.section(&code);
    Ok(module.finish())
}

fn response_handle(operation: ServerOperation) -> Function {
    // Parameters 0/1 are own request/outparam; locals 2/3/4 are response/body/status.
    let mut handle = Function::new([(3, ValType::I32)]);
    if operation == ServerOperation::ClockRead {
        handle.instruction(&Instruction::Call(8));
        handle.instruction(&Instruction::Drop);
    }
    handle.instruction(&Instruction::Call(7));
    handle.instruction(&Instruction::LocalTee(4));
    handle.instruction(&Instruction::I32Const(200));
    handle.instruction(&Instruction::I32LtU);
    trap_if(&mut handle);
    handle.instruction(&Instruction::LocalGet(4));
    handle.instruction(&Instruction::I32Const(599));
    handle.instruction(&Instruction::I32GtU);
    trap_if(&mut handle);
    handle.instruction(&Instruction::Call(0));
    handle.instruction(&Instruction::Call(1));
    handle.instruction(&Instruction::LocalTee(2));
    handle.instruction(&Instruction::LocalGet(4));
    handle.instruction(&Instruction::Call(2));
    trap_if(&mut handle);
    handle.instruction(&Instruction::LocalGet(2));
    handle.instruction(&Instruction::I32Const(0));
    handle.instruction(&Instruction::Call(3));
    check_result(&mut handle, 0);
    handle.instruction(&Instruction::I32Const(4));
    handle.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
    handle.instruction(&Instruction::LocalSet(3));
    handle.instruction(&Instruction::LocalGet(3));
    handle.instruction(&Instruction::I32Const(0)); // None trailers.
    handle.instruction(&Instruction::I32Const(0));
    handle.instruction(&Instruction::I32Const(16));
    handle.instruction(&Instruction::Call(4));
    check_result(&mut handle, 16);
    handle.instruction(&Instruction::LocalGet(1));
    handle.instruction(&Instruction::I32Const(0)); // Ok outgoing-response.
    handle.instruction(&Instruction::LocalGet(2));
    handle.instruction(&Instruction::I32Const(0));
    handle.instruction(&Instruction::I64Const(0));
    for _ in 0..4 {
        handle.instruction(&Instruction::I32Const(0));
    }
    handle.instruction(&Instruction::Call(5));
    handle.instruction(&Instruction::LocalGet(0));
    handle.instruction(&Instruction::Call(6));
    handle.instruction(&Instruction::End);
    handle
}

fn check_result(function: &mut Function, pointer: i32) {
    function.instruction(&Instruction::I32Const(pointer));
    function.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));
    trap_if(function);
}
