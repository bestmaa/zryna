//! Independent pinned validation and exhaustive capability/signature/index audit of final bytes.

use super::{MAX_BYTES, budget, layout::Layout};
use crate::ValidatedWebAssemblyArtifact;
use wasmparser::{
    Encoding, ExternalKind, Operator, Parser, Payload, ValType, Validator, WasmFeatures,
};
use zryna_diagnostics::Diagnostic;

pub(super) fn seal(
    bytes: Vec<u8>,
    layout: &Layout<'_, '_>,
) -> Result<ValidatedWebAssemblyArtifact, Diagnostic> {
    if bytes.len() > MAX_BYTES {
        return Err(budget());
    }
    Validator::new_with_features(WasmFeatures::WASM1)
        .validate_all(&bytes)
        .map_err(|error| error_diagnostic("ZRYNA-W4004", error))?;
    audit(&bytes, layout)?;
    Ok(ValidatedWebAssemblyArtifact { bytes })
}

#[allow(
    clippy::too_many_lines,
    reason = "The independent parser exhaustively checks one closed module inventory."
)]
fn audit(bytes: &[u8], layout: &Layout<'_, '_>) -> Result<(), Diagnostic> {
    let private = layout.functions.len();
    let total = private + layout.program.export_functions().len();
    let mut sections = Vec::new();
    let mut bodies = 0;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(parse_error)? {
            Payload::Version { num: 1, encoding: Encoding::Module, .. } | Payload::End(_) => {}
            Payload::TypeSection(reader) => {
                sections.push(1);
                if reader.count() as usize != total + 3 {
                    return Err(reject("changed type count"));
                }
                for (index, ty) in reader.into_iter_err_on_gc_types().enumerate() {
                    let ty = ty.map_err(parse_error)?;
                    let (parameters, results) = if index < 3 {
                        ([3, 4, 3][index], 1)
                    } else if index < private + 3 {
                        (layout.functions[index - 3].parameters as usize, 1)
                    } else {
                        let function = layout.program.export_functions()[index - private - 3];
                        (layout.program.functions()[function].parameters.len(), 1)
                    };
                    if ty.params().len() != parameters
                        || ty.results().len() != results
                        || ty.params().iter().chain(ty.results()).any(|ty| *ty != ValType::I32)
                    {
                        return Err(reject("changed private/public scalar signature"));
                    }
                }
            }
            Payload::ImportSection(reader) => {
                sections.push(2);
                let imports =
                    reader.into_imports().collect::<Result<Vec<_>, _>>().map_err(parse_error)?;
                if imports.len() != 4 {
                    return Err(reject("changed import count"));
                }
                for (index, import) in imports.iter().enumerate() {
                    if import.module != super::runtime::NAMESPACE {
                        return Err(reject("foreign runtime namespace"));
                    }
                    if index < 3 {
                        if import.name != super::runtime::NAMES[index]
                            || !matches!(import.ty,wasmparser::TypeRef::Func(ty) if ty as usize==index)
                        {
                            return Err(reject("changed runtime call declaration"));
                        }
                    } else {
                        let wasmparser::TypeRef::Memory(memory) = import.ty else {
                            return Err(reject("missing trusted memory"));
                        };
                        if import.name != "memory"
                            || memory.initial != 1024
                            || memory.maximum != Some(1024)
                            || memory.memory64
                            || memory.shared
                            || memory.page_size_log2.is_some()
                        {
                            return Err(reject("changed bounded runtime memory"));
                        }
                    }
                }
                for (name, arity) in super::runtime::NAMES.iter().zip([3, 4, 3]) {
                    let declaration = layout
                        .program
                        .runtime()
                        .declarations()
                        .webassembly
                        .iter()
                        .find(|d| d.operation == *name)
                        .ok_or_else(|| reject("missing runtime authority"))?;
                    if declaration.parameters.len() != arity
                        || declaration
                            .parameters
                            .iter()
                            .chain(&declaration.results)
                            .any(|v| *v != zryna_ownership_runtime_abi::raw::WebAssemblyLane::I32)
                        || declaration.results.len() != 1
                    {
                        return Err(reject("foreign runtime physical declaration"));
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                sections.push(3);
                if reader.count() as usize != total {
                    return Err(reject("changed function count"));
                }
                for (index, ty) in reader.into_iter().enumerate() {
                    if ty.map_err(parse_error)? as usize != index + 3 {
                        return Err(reject("changed function type index"));
                    }
                }
            }
            Payload::GlobalSection(reader) => {
                sections.push(6);
                if reader.count() != layout.return_lanes + 2 {
                    return Err(reject("changed private return lane count"));
                }
                for global in reader {
                    let global = global.map_err(parse_error)?;
                    let mut initial = global.init_expr.get_operators_reader();
                    if global.ty.content_type != ValType::I32
                        || !global.ty.mutable
                        || global.ty.shared
                        || !matches!(initial.read(), Ok(Operator::I32Const { value: 0 }))
                        || !matches!(initial.read(), Ok(Operator::End))
                        || !initial.eof()
                    {
                        return Err(reject("changed private zero-initialized return lane"));
                    }
                }
            }
            Payload::ExportSection(reader) => {
                sections.push(7);
                if reader.count() as usize != total - private {
                    return Err(reject("changed scalar export count"));
                }
                for (index, (actual, expected)) in
                    reader.into_iter().zip(layout.program.scalar_abi().exports()).enumerate()
                {
                    let actual = actual.map_err(parse_error)?;
                    if actual.kind != ExternalKind::Func
                        || actual.index as usize != private + index + 3
                        || actual.name != expected.webassembly_name().as_str()
                    {
                        return Err(reject("changed scalar ABI export identity or private export"));
                    }
                }
            }
            Payload::CodeSectionStart { count, .. } => {
                sections.push(10);
                if count as usize != total {
                    return Err(reject("changed code body count"));
                }
            }
            Payload::CodeSectionEntry(body) => {
                if bodies >= total {
                    return Err(reject("extra code body"));
                }
                audit_body(&body, bodies, layout)?;
                bodies += 1;
            }
            Payload::DataSection(reader) => {
                sections.push(11);
                if reader.count() as usize != layout.literals.len() {
                    return Err(reject("changed literal count"));
                }
                for (actual, (offset, expected)) in reader.into_iter().zip(layout.literals.values())
                {
                    let actual = actual.map_err(parse_error)?;
                    let wasmparser::DataKind::Active { memory_index: 0, offset_expr } = actual.kind
                    else {
                        return Err(reject("changed literal memory"));
                    };
                    let mut ops = offset_expr.get_operators_reader();
                    if !matches!(ops.read(),Ok(Operator::I32Const{value}) if value==i32::try_from(*offset).map_err(|_|budget())?)
                        || !matches!(ops.read(), Ok(Operator::End))
                        || !ops.eof()
                        || actual.data != expected
                    {
                        return Err(reject("changed exact source UTF8"));
                    }
                }
            }
            _ => return Err(reject("unapproved core section or ambient capability")),
        }
    }
    if sections != [1, 2, 3, 6, 7, 10, 11] || bodies != total {
        return Err(reject("incomplete or reordered exact section inventory"));
    }
    Ok(())
}

fn audit_body(
    body: &wasmparser::FunctionBody<'_>,
    index: usize,
    layout: &Layout<'_, '_>,
) -> Result<(), Diagnostic> {
    let private = layout.functions.len();
    let (parameters, declared) = if index < private {
        let locals = &layout.functions[index];
        (locals.parameters, locals.declared)
    } else {
        let function = layout.program.export_functions()[index - private];
        (
            u32::try_from(layout.program.functions()[function].parameters.len())
                .map_err(|_| budget())?,
            1,
        )
    };
    let mut locals = body.get_locals_reader().map_err(parse_error)?;
    if declared == 0 {
        if locals.get_count() != 0 {
            return Err(reject("extra wrapper locals"));
        }
    } else if locals.get_count() != 1
        || locals.read().map_err(parse_error)? != (declared, ValType::I32)
    {
        return Err(reject("changed sealed function-local inventory"));
    }
    let maximum = parameters.checked_add(declared).ok_or_else(budget)?;
    let mut operators = body.get_operators_reader().map_err(parse_error)?;
    while !operators.eof() {
        operator(&operators.read().map_err(parse_error)?, maximum, layout)?;
    }
    Ok(())
}

fn operator(
    operation: &Operator<'_>,
    locals: u32,
    layout: &Layout<'_, '_>,
) -> Result<(), Diagnostic> {
    match operation {
        Operator::LocalGet { local_index } | Operator::LocalSet { local_index }
            if *local_index < locals =>
        {
            Ok(())
        }
        Operator::GlobalGet { global_index } | Operator::GlobalSet { global_index }
            if *global_index < layout.return_lanes + 2 =>
        {
            Ok(())
        }
        Operator::Call { function_index }
            if (*function_index as usize) < layout.functions.len() + 3 =>
        {
            Ok(())
        }
        Operator::Block { blockty: wasmparser::BlockType::Empty }
        | Operator::Loop { blockty: wasmparser::BlockType::Empty }
        | Operator::If { blockty: wasmparser::BlockType::Empty }
        | Operator::I32Const { .. }
        | Operator::I32Load {
            memarg: wasmparser::MemArg { memory: 0, align: 2, max_align: 2, offset: 0 | 4 | 8 },
        }
        | Operator::I32Mul
        | Operator::I32GeU
        | Operator::I32Eqz
        | Operator::I32Add
        | Operator::I32Eq
        | Operator::I32Ne
        | Operator::I32Or
        | Operator::Br { .. }
        | Operator::BrIf { .. }
        | Operator::Else
        | Operator::End
        | Operator::Return
        | Operator::Unreachable => Ok(()),
        _ => Err(reject("unapproved instruction or private index")),
    }
}

fn parse_error(error: wasmparser::BinaryReaderError) -> Diagnostic {
    error_diagnostic("ZRYNA-W4004", error)
}
fn reject(reason: &str) -> Diagnostic {
    error_diagnostic("ZRYNA-W4003", reason)
}
fn error_diagnostic(code: &str, reason: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error(
        code,
        None,
        format!("generic owned Wasm final-byte audit: {reason}"),
        "emit only the exact sealed scalar-lane core contract",
    )
}
