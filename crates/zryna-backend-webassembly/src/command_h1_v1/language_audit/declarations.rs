use super::invalid;
use wasmparser::{
    GlobalSectionReader, ImportSectionReader, Operator, TypeRef, TypeSectionReader, ValType,
};
use zryna_diagnostics::Diagnostic;
use zryna_ir::{command_h1_v1::VerifiedProgram, data_ownership_v1::VerifiedModule};

pub(super) struct Shape {
    arities: Vec<usize>,
    pub(super) declarations: Vec<u32>,
    pub(super) environment: Option<u32>,
    pub(super) environment_caller: Option<u32>,
    pub(super) main: u32,
    pub(super) run: u32,
    pub(super) type_count: u32,
    pub(super) program_base: u32,
}

impl Shape {
    pub(super) fn derive(program: &VerifiedProgram) -> Result<Self, Diagnostic> {
        let functions = program.modules().flat_map(VerifiedModule::functions).collect::<Vec<_>>();
        let mut arities = vec![1];
        let mut exports = Vec::new();
        for (index, function) in functions.iter().enumerate() {
            let arity = function.parameters().len() + function.borrow_parameters().len();
            if !arities.contains(&arity) {
                arities.push(arity);
            }
            if function.public_export().is_some() {
                exports.push(index);
            }
        }
        if !arities.contains(&3) {
            arities.push(3);
        }
        let [entry] = exports.as_slice() else {
            return Err(invalid());
        };
        if functions[*entry].parameters().len() != 0
            || functions[*entry].borrow_parameters().len() != 0
        {
            return Err(invalid());
        }
        let count =
            u32::try_from(program.linear32_layouts().types().len()).map_err(|_| invalid())?;
        let environment = program.source().environment().map(|_| 6 + count * 2);
        let base = 6 + count * 2 + u32::from(environment.is_some());
        let main = base + u32::try_from(*entry).map_err(|_| invalid())?;
        let mut environment_caller = None;
        for (index, function) in functions.iter().enumerate() {
            for instruction in
                function.blocks().flat_map(zryna_ir::data_ownership_v1::VerifiedBlock::instructions)
            {
                if instruction.kind()
                    == zryna_ir::data_ownership_v1::VerifiedInstructionKind::EnvironmentLookup
                    && environment_caller
                        .replace(base + u32::try_from(index).map_err(|_| invalid())?)
                        .is_some()
                {
                    return Err(invalid());
                }
            }
        }
        if environment.is_some() != environment_caller.is_some() {
            return Err(invalid());
        }
        let run = base + u32::try_from(functions.len()).map_err(|_| invalid())?;
        let drop_type = u32::try_from(arities.len()).map_err(|_| invalid())?;
        let mut declarations = vec![0; count as usize];
        declarations.extend(vec![drop_type; count as usize]);
        if environment.is_some() {
            declarations.push(type_for(&arities, 0)?);
        }
        for function in &functions {
            declarations.push(type_for(
                &arities,
                function.parameters().len() + function.borrow_parameters().len(),
            )?);
        }
        declarations.extend([type_for(&arities, 0)?, 0, drop_type]);
        Ok(Self {
            arities,
            declarations,
            environment,
            environment_caller,
            main,
            run,
            type_count: count,
            program_base: base,
        })
    }

    pub(super) fn globals(globals: GlobalSectionReader<'_>) -> Result<(), Diagnostic> {
        if globals.count() != 4 {
            return Err(invalid());
        }
        for global in globals {
            let global = global.map_err(|_| invalid())?;
            let mut initial = global.init_expr.get_operators_reader();
            if global.ty.content_type != ValType::I32
                || !global.ty.mutable
                || global.ty.shared
                || !matches!(
                    initial.read().map_err(|_| invalid())?,
                    Operator::I32Const { value: 0 }
                )
                || !matches!(initial.read().map_err(|_| invalid())?, Operator::End)
                || !initial.eof()
            {
                return Err(invalid());
            }
        }
        Ok(())
    }

    pub(super) fn types(&self, types: TypeSectionReader<'_>) -> Result<(), Diagnostic> {
        if types.count() as usize != self.arities.len() + 3 {
            return Err(invalid());
        }
        for (index, ty) in types.into_iter_err_on_gc_types().enumerate() {
            let ty = ty.map_err(|_| invalid())?;
            let (parameters, results) = if index < self.arities.len() {
                (self.arities[index], 1)
            } else {
                (
                    match index - self.arities.len() {
                        0 => 1,
                        1 => 3,
                        _ => 0,
                    },
                    0,
                )
            };
            if ty.params() != vec![ValType::I32; parameters]
                || ty.results() != vec![ValType::I32; results]
            {
                return Err(invalid());
            }
        }
        Ok(())
    }

    pub(super) fn imports(&self, imports: ImportSectionReader<'_>) -> Result<(), Diagnostic> {
        let imports =
            imports.into_imports().collect::<Result<Vec<_>, _>>().map_err(|_| invalid())?;
        if imports.len() != 13 {
            return Err(invalid());
        }
        let drop_type = u32::try_from(self.arities.len()).map_err(|_| invalid())?;
        for (index, (namespace, name, expected_type)) in [
            ("storage", "allocate", 0),
            ("storage", "copy", drop_type + 1),
            ("host", "get-environment", drop_type),
            ("storage", "validate", type_for(&self.arities, 3)?),
            ("storage", "drain", drop_type + 2),
            ("storage", "canonical-state", 0),
        ]
        .into_iter()
        .enumerate()
        {
            let import = &imports[index];
            if import.module != namespace
                || import.name != name
                || !matches!(import.ty, TypeRef::Func(actual) if actual == expected_type)
            {
                return Err(invalid());
            }
        }
        let import = &imports[6];
        if import.module != "storage"
            || import.name != "memory"
            || !matches!(import.ty, TypeRef::Memory(memory)
            if memory.initial == 256 && memory.maximum == Some(256) && !memory.memory64
            && !memory.shared && memory.page_size_log2.is_none())
        {
            return Err(invalid());
        }
        for (index, name) in
            ["arena", "status", "drops", "live", "peak", "references"].into_iter().enumerate()
        {
            let import = &imports[7 + index];
            if import.module != "storage"
                || import.name != name
                || !matches!(import.ty, TypeRef::Global(global)
                if global.content_type == ValType::I32 && global.mutable && !global.shared)
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

fn type_for(arities: &[usize], arity: usize) -> Result<u32, Diagnostic> {
    arities
        .iter()
        .position(|value| *value == arity)
        .and_then(|index| u32::try_from(index).ok())
        .ok_or_else(invalid)
}
