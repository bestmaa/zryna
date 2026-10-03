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
                if reader.count() as usize != total {
                    return Err(reject("changed type count"));
                }
                for (index, ty) in reader.into_iter_err_on_gc_types().enumerate() {
                    let ty = ty.map_err(parse_error)?;
                    let (parameters, results) = if index < private {
                        (layout.functions[index].parameters as usize, 0)
                    } else {
                        let function = layout.program.export_functions()[index - private];
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
            Payload::FunctionSection(reader) => {
                sections.push(3);
                if reader.count() as usize != total {
                    return Err(reject("changed function count"));
                }
                for (index, ty) in reader.into_iter().enumerate() {
                    if ty.map_err(parse_error)? as usize != index {
                        return Err(reject("changed function type index"));
                    }
                }
            }
            Payload::GlobalSection(reader) => {
                sections.push(6);
                if reader.count() != layout.return_lanes {
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
                        || actual.index as usize != private + index
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
            _ => return Err(reject("unapproved core section or ambient capability")),
        }
    }
    if sections != [1, 3, 6, 7, 10] || bodies != total {
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
            0,
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
            if *global_index < layout.return_lanes =>
        {
            Ok(())
        }
        Operator::Call { function_index }
            if (*function_index as usize) < layout.functions.len() =>
        {
            Ok(())
        }
        Operator::Block { blockty: wasmparser::BlockType::Empty }
        | Operator::Loop { blockty: wasmparser::BlockType::Empty }
        | Operator::If { blockty: wasmparser::BlockType::Empty }
        | Operator::I32Const { .. }
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
        format!("generic Copy Wasm final-byte audit: {reason}"),
        "emit only the exact sealed scalar-lane core contract",
    )
}
