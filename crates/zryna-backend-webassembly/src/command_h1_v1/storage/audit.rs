use wasmparser::{
    Encoding, ExternalKind, Operator, Parser, Payload, ValType, Validator, WasmFeatures,
};
use zryna_diagnostics::Diagnostic;

use super::{CANONICAL_START, LANGUAGE_START};
mod shape;

pub(in crate::command_h1_v1) fn audit(bytes: &[u8]) -> Result<(), Diagnostic> {
    if bytes.len() > 16_384 {
        return Err(invalid("storage core exceeds 16384 bytes"));
    }
    Validator::new_with_features(WasmFeatures::WASM1)
        .validate_all(bytes)
        .map_err(|_| invalid("storage core is not valid WebAssembly 1.0"))?;
    let mut sections = [false; 6];
    let mut bodies = 0_usize;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(malformed)? {
            Payload::Version { encoding: Encoding::Module, .. } | Payload::End(_) => {}
            Payload::TypeSection(types) if !sections[0] && types.count() == 7 => {
                sections[0] = true;
                for (index, ty) in types.into_iter_err_on_gc_types().enumerate() {
                    let ty = ty.map_err(malformed)?;
                    let (parameters, results) =
                        [(1, 1), (3, 0), (4, 1), (3, 1), (0, 0), (2, 1), (1, 0)][index];
                    if ty.params() != vec![ValType::I32; parameters]
                        || ty.results() != vec![ValType::I32; results]
                    {
                        return Err(invalid("storage core function signature differs"));
                    }
                }
            }
            Payload::FunctionSection(functions) if !sections[1] && functions.count() == 9 => {
                sections[1] = true;
                for (actual, expected) in functions.into_iter().zip([0, 1, 2, 3, 4, 5, 5, 6, 0]) {
                    if actual.map_err(malformed)? != expected {
                        return Err(invalid("storage core function declaration differs"));
                    }
                }
            }
            Payload::MemorySection(memories) if !sections[2] && memories.count() == 1 => {
                sections[2] = true;
                for memory in memories {
                    let memory = memory.map_err(malformed)?;
                    if memory.initial != 256
                        || memory.maximum != Some(256)
                        || memory.memory64
                        || memory.shared
                        || memory.page_size_log2.is_some()
                    {
                        return Err(invalid("storage core must own one fixed 256-page memory"));
                    }
                }
            }
            Payload::GlobalSection(globals) if !sections[3] && globals.count() == 10 => {
                sections[3] = true;
                for (index, global) in globals.into_iter().enumerate() {
                    let global = global.map_err(malformed)?;
                    let expected = match index {
                        0 => LANGUAGE_START,
                        6 => CANONICAL_START,
                        _ => 0,
                    };
                    let mut init = global.init_expr.get_operators_reader();
                    if global.ty.content_type != ValType::I32
                        || !global.ty.mutable
                        || global.ty.shared
                        || !matches!(init.read(), Ok(Operator::I32Const { value }) if value == expected)
                        || !matches!(init.read(), Ok(Operator::End))
                        || !init.eof()
                    {
                        return Err(invalid(
                            "storage partition, ledger or language global differs",
                        ));
                    }
                }
            }
            Payload::ExportSection(exports) if !sections[4] && exports.count() == 13 => {
                sections[4] = true;
                audit_exports(exports)?;
            }
            Payload::CodeSectionStart { count: 9, .. } if !sections[5] => sections[5] = true,
            Payload::CodeSectionEntry(body) if bodies < 9 => {
                audit_body(&body, bodies)?;
                bodies += 1;
            }
            _ => return Err(invalid("storage core contains an extra or unapproved section")),
        }
    }
    if bodies != 9 || sections.iter().any(|seen| !seen) {
        return Err(invalid("storage core is incomplete"));
    }
    Ok(())
}

fn audit_exports(exports: wasmparser::ExportSectionReader<'_>) -> Result<(), Diagnostic> {
    let expected = [
        ("memory", ExternalKind::Memory, 0),
        ("arena", ExternalKind::Global, 0),
        ("status", ExternalKind::Global, 1),
        ("drops", ExternalKind::Global, 2),
        ("live", ExternalKind::Global, 3),
        ("peak", ExternalKind::Global, 4),
        ("references", ExternalKind::Global, 5),
        ("allocate", ExternalKind::Func, 0),
        ("copy", ExternalKind::Func, 1),
        ("realloc", ExternalKind::Func, 2),
        ("validate", ExternalKind::Func, 3),
        ("drain", ExternalKind::Func, 4),
        ("canonical-state", ExternalKind::Func, 8),
    ];
    for (actual, (name, kind, index)) in exports.into_iter().zip(expected) {
        let actual = actual.map_err(malformed)?;
        if actual.name != name || actual.kind != kind || actual.index != index {
            return Err(invalid(
                "storage export binding differs or exposes canonical mutable state",
            ));
        }
    }
    Ok(())
}

fn audit_body(body: &wasmparser::FunctionBody<'_>, role: usize) -> Result<(), Diagnostic> {
    shape::audit(body, role)?;
    let locals = body.get_locals_reader().map_err(malformed)?;
    let expected = [2, 1, 2, 1, 1, 1, 3, 0, 0][role];
    if locals.get_count() != u32::from(expected != 0) {
        return Err(invalid("storage helper local groups differ"));
    }
    for local in locals {
        if local.map_err(malformed)? != (expected, ValType::I32) {
            return Err(invalid("storage helper local type or count differs"));
        }
    }
    let mut operators = body.get_operators_reader().map_err(malformed)?;
    while !operators.eof() {
        let operator = operators.read().map_err(malformed)?;
        let approved = match operator {
            Operator::GlobalGet { global_index } => match role {
                0 => [0, 1, 9].contains(&global_index),
                1..=5 => global_index == 9,
                6 | 8 => (6..=9).contains(&global_index),
                7 => [7, 9].contains(&global_index),
                _ => false,
            },
            Operator::GlobalSet { global_index } => match role {
                0 => global_index <= 1,
                1..=3 => global_index == 9,
                4 | 7 => [6, 7].contains(&global_index),
                6 => (6..=9).contains(&global_index),
                _ => false,
            },
            Operator::Call { function_index } => {
                role == 2 && [1, 5, 6, 7].contains(&function_index)
            }
            Operator::I32Load { memarg } => [2, 3, 5, 6].contains(&role) && word(memarg),
            Operator::I32Store { memarg } => [4, 6, 7].contains(&role) && word(memarg),
            Operator::I32Load8U { memarg } | Operator::I32Store8 { memarg } => {
                role == 1 && memarg.memory == 0 && memarg.align == 0 && memarg.offset == 0
            }
            Operator::Unreachable
            | Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::If { .. }
            | Operator::Else
            | Operator::End
            | Operator::Br { .. }
            | Operator::BrIf { .. }
            | Operator::Return
            | Operator::LocalGet { .. }
            | Operator::LocalSet { .. }
            | Operator::LocalTee { .. }
            | Operator::I32Const { .. }
            | Operator::I32Eqz
            | Operator::I32Eq
            | Operator::I32Ne
            | Operator::I32LtU
            | Operator::I32GtU
            | Operator::I32GeU
            | Operator::I32Add
            | Operator::I32Sub
            | Operator::I32And => true,
            _ => false,
        };
        if !approved {
            return Err(invalid("storage helper exceeds its memory, state or direct-call role"));
        }
    }
    Ok(())
}

fn word(memory: wasmparser::MemArg) -> bool {
    memory.memory == 0 && memory.align == 2 && [0, 4, 8].contains(&memory.offset)
}

fn malformed(_: wasmparser::BinaryReaderError) -> Diagnostic {
    invalid("storage core syntax is malformed")
}

fn invalid(message: &str) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4101",
        None,
        message,
        "restore the fixed command storage core and memory partition",
    )
}
