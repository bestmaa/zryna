use super::{
    CommandContext, Context, MEMORY_PAGES, allocator, command_environment, copy_memory,
    encode_function, export_wrapper, index_error, memory, observation,
};
use wasm_encoder::{
    CodeSection, ConstExpr, ExportKind, ExportSection, FunctionSection, GlobalSection, GlobalType,
    MemorySection, MemoryType, Module, TypeSection, ValType,
};
use zryna_ir::data_ownership_v1::{VerifiedFunction, VerifiedProgram};
mod command_imports;
mod command_run;

pub(super) fn module(program: &VerifiedProgram) -> Result<Vec<u8>, zryna_diagnostics::Diagnostic> {
    let functions = program
        .modules()
        .flat_map(zryna_ir::data_ownership_v1::VerifiedModule::functions)
        .collect::<Vec<_>>();
    encode(&functions, program.linear32_layouts(), None)
}

pub(super) fn command(
    program: &zryna_ir::command_h1_v1::VerifiedProgram,
) -> Result<Vec<u8>, zryna_diagnostics::Diagnostic> {
    let functions = program
        .modules()
        .flat_map(zryna_ir::data_ownership_v1::VerifiedModule::functions)
        .collect::<Vec<_>>();
    encode(&functions, program.linear32_layouts(), Some(program))
}

#[allow(clippy::too_many_lines)]
fn encode(
    functions: &[VerifiedFunction<'_>],
    layouts: &zryna_layout::VerifiedLayouts,
    command: Option<&zryna_ir::command_h1_v1::VerifiedProgram>,
) -> Result<Vec<u8>, zryna_diagnostics::Diagnostic> {
    let type_count = u32::try_from(layouts.types().len()).map_err(|_| index_error())?;
    let clone_storage = super::clone_frontier::required(functions);
    let clone_frontier = command.is_some() && clone_storage;
    let mut arities = vec![1_usize];
    for function in functions {
        let arity = function.parameters().len() + function.borrow_parameters().len();
        if !arities.contains(&arity) {
            arities.push(arity);
        }
    }
    if command.is_some() && !arities.contains(&3) {
        arities.push(3);
    }
    let mut module = Module::new();
    let mut types = TypeSection::new();
    for arity in &arities {
        types.ty().function(vec![ValType::I32; *arity], [ValType::I32]);
    }
    let drop_type = u32::try_from(arities.len()).map_err(|_| index_error())?;
    types.ty().function([ValType::I32], []);
    let copy_type = drop_type + 1;
    types.ty().function([ValType::I32, ValType::I32, ValType::I32], []);
    let drain_type = copy_type + 1;
    if command.is_some() {
        types.ty().function([], []);
    }
    module.section(&types);

    if command.is_some() {
        command_imports::encode(&mut module, &arities, drop_type, copy_type, drain_type)?;
    }

    let mut declarations = FunctionSection::new();
    if command.is_none() {
        declarations.function(0);
        declarations.function(copy_type);
    }
    for _ in 0..type_count {
        declarations.function(0);
    }
    for _ in 0..type_count {
        declarations.function(drop_type);
    }
    let helper_base = if command.is_some() { 6 } else { 2 };
    let environment_index = helper_base + type_count * 2;
    let environment = command.and_then(|program| program.source().environment());
    if environment.is_some() {
        declarations.function(
            u32::try_from(arities.iter().position(|arity| *arity == 0).ok_or_else(index_error)?)
                .map_err(|_| index_error())?,
        );
    }
    for function in functions {
        let arity = function.parameters().len() + function.borrow_parameters().len();
        declarations.function(
            u32::try_from(arities.iter().position(|candidate| *candidate == arity).unwrap_or(0))
                .map_err(|_| index_error())?,
        );
    }
    for function in functions.iter().filter(|function| function.public_export().is_some()) {
        let arity = function.parameters().len() + function.borrow_parameters().len();
        declarations.function(
            u32::try_from(arities.iter().position(|candidate| *candidate == arity).unwrap_or(0))
                .map_err(|_| index_error())?,
        );
    }
    declarations.function(0);
    declarations.function(drop_type);
    module.section(&declarations);

    if command.is_none() {
        let mut memories = MemorySection::new();
        memories.memory(MemoryType {
            minimum: MEMORY_PAGES,
            maximum: Some(MEMORY_PAGES),
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        module.section(&memories);
        let mut globals = GlobalSection::new();
        globals.global(
            GlobalType { val_type: ValType::I32, mutable: true, shared: false },
            &ConstExpr::i32_const(observation::ARENA_START),
        );
        for _ in 0..if clone_storage { 9 } else { 5 } {
            globals.global(
                GlobalType { val_type: ValType::I32, mutable: true, shared: false },
                &ConstExpr::i32_const(0),
            );
        }
        module.section(&globals);
    }

    let mut private_globals = GlobalSection::new();
    for _ in 0..4 {
        private_globals.global(
            GlobalType { val_type: ValType::I32, mutable: true, shared: false },
            &ConstExpr::i32_const(0),
        );
    }
    if command.is_some() && clone_frontier {
        module.section(&private_globals);
    }

    let mut exports = ExportSection::new();
    let program_base = environment_index + u32::from(environment.is_some());
    let mut wrapper_index =
        program_base + u32::try_from(functions.len()).map_err(|_| index_error())?;
    for function in functions {
        if let Some(export) = function.public_export() {
            exports.export(
                if command.is_some() { "run" } else { export.webassembly_name().as_str() },
                ExportKind::Func,
                wrapper_index,
            );
            wrapper_index += 1;
        }
    }
    exports.export("$zryna$observation", ExportKind::Func, wrapper_index);
    module.section(&exports);

    let context = Context {
        functions,
        layouts,
        type_count,
        program_base,
        observation: wrapper_index,
        helper_base,
        clone_frontier,
        command: environment.map(|requirement| CommandContext {
            key: requirement.key(),
            environment: environment_index,
        }),
    };
    let mut code = CodeSection::new();
    if command.is_none() {
        code.function(&allocator());
        code.function(&copy_memory());
    }
    for layout in layouts.types() {
        code.function(&memory::clone_helper(layout, &context)?);
    }
    for layout in layouts.types() {
        code.function(&memory::drop_helper(layout, &context)?);
    }
    if let Some(requirement) = environment {
        let result_type = functions
            .iter()
            .flat_map(|function| function.blocks())
            .flat_map(zryna_ir::data_ownership_v1::VerifiedBlock::instructions)
            .find(|instruction| {
                instruction.kind()
                    == zryna_ir::data_ownership_v1::VerifiedInstructionKind::EnvironmentLookup
            })
            .and_then(zryna_ir::data_ownership_v1::VerifiedInstruction::result_type)
            .and_then(|ty| layouts.type_by_id(ty))
            .ok_or_else(index_error)?;
        code.function(&command_environment::helper(requirement.key(), result_type)?);
    }
    for function in functions {
        code.function(&encode_function(*function, &context)?);
    }
    for (index, function) in functions.iter().enumerate() {
        if function.public_export().is_some() {
            let target = program_base + u32::try_from(index).map_err(|_| index_error())?;
            if command.is_some() {
                code.function(&command_run::wrapper(target, clone_frontier));
            } else {
                code.function(&export_wrapper(*function, target, clone_frontier)?);
            }
        }
    }
    code.function(&observation::getter());
    code.function(&observation::recorder());
    module.section(&code);
    Ok(module.finish())
}
