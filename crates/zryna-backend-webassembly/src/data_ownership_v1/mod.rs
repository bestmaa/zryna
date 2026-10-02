use wasmparser::{
    Encoding, ExternalKind, Operator, Parser, Payload, ValType, Validator, WasmFeatures,
};
use zryna_diagnostics::Diagnostic;
use zryna_ir::data_ownership_v1::{VerifiedFunction, VerifiedProgram};
use zryna_ownership_runtime_abi::VerifiedOwnershipRuntimeAbi;

use crate::ValidatedWebAssemblyArtifact;

pub(crate) mod encode;

const MAX_BYTES: usize = 32 * 1024 * 1024;

/// Emits deterministic memory-bearing core WebAssembly from sealed `DataOwnershipV1` authority.
///
/// # Errors
/// Returns a stable diagnostic if authorities disagree, encoding exceeds its bound, or the
/// completed module fails the independent capability audit.
pub fn emit_data_ownership(
    program: &VerifiedProgram,
    runtime_abi: &VerifiedOwnershipRuntimeAbi,
) -> Result<ValidatedWebAssemblyArtifact, Diagnostic> {
    validate_authority(program, runtime_abi)?;
    let bytes = encode::module(program)?;
    if bytes.len() > MAX_BYTES {
        return Err(error(
            "ZRYNA-W3003",
            format!("DataOwnershipV1 module exceeds {MAX_BYTES} bytes"),
        ));
    }
    validate(&bytes)?;
    audit(&bytes, program)?;
    Ok(ValidatedWebAssemblyArtifact { bytes })
}

fn validate_authority(
    program: &VerifiedProgram,
    runtime_abi: &VerifiedOwnershipRuntimeAbi,
) -> Result<(), Diagnostic> {
    if program.type_universe_identity() != runtime_abi.type_universe_identity()
        || program.linear32_layouts().fingerprint() != &runtime_abi.linear32_fingerprint()
        || program.linux_x86_64_layouts().fingerprint() != &runtime_abi.linux_x86_64_fingerprint()
        || runtime_abi.webassembly_functions().len() != runtime_abi.operations().len()
    {
        return Err(error(
            "ZRYNA-W3001",
            "ownership runtime ABI is not bound to the verified program",
        ));
    }
    Ok(())
}

fn validate(bytes: &[u8]) -> Result<(), Diagnostic> {
    let mut validator = Validator::new_with_features(WasmFeatures::WASM1);
    validator.validate_all(bytes).map(|_| ()).map_err(|failure| {
        error("ZRYNA-W3002", format!("completed DataOwnershipV1 module is invalid: {failure}"))
    })
}

#[allow(clippy::too_many_lines)]
fn audit(bytes: &[u8], program: &VerifiedProgram) -> Result<(), Diagnostic> {
    let expected_globals = private_global_count(program);
    let mut memories = 0_u32;
    let mut globals = 0_u32;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|failure| error("ZRYNA-W3002", failure.to_string()))? {
            Payload::Version { encoding: Encoding::Module, .. }
            | Payload::TypeSection(_)
            | Payload::FunctionSection(_)
            | Payload::ExportSection(_)
            | Payload::CodeSectionStart { .. }
            | Payload::CodeSectionEntry(_)
            | Payload::End(_) => {}
            Payload::MemorySection(section) => {
                memories += section.count();
                let mut entries = section.into_iter();
                let exact = entries
                    .next()
                    .transpose()
                    .map_err(|failure| error("ZRYNA-W3002", failure.to_string()))?;
                if memories != 1
                    || !exact.is_some_and(|memory| {
                        memory.initial == 256
                            && memory.maximum == Some(256)
                            && !memory.memory64
                            && !memory.shared
                            && memory.page_size_log2.is_none()
                    })
                    || entries.next().is_some()
                {
                    return Err(error(
                        "ZRYNA-W3004",
                        "module must contain exactly one private fixed 256-page Linear32 memory",
                    ));
                }
            }
            Payload::GlobalSection(section) => {
                globals += section.count();
                for (index, global) in section.into_iter().enumerate() {
                    let global =
                        global.map_err(|failure| error("ZRYNA-W3002", failure.to_string()))?;
                    let mut init = global.init_expr.get_operators_reader();
                    let expected = if index == 0 { 65_536 } else { 0 };
                    if index >= expected_globals as usize
                        || global.ty.content_type != ValType::I32
                        || !global.ty.mutable
                        || global.ty.shared
                        || !matches!(init.read(), Ok(Operator::I32Const { value }) if value == expected)
                        || !matches!(init.read(), Ok(Operator::End))
                        || !init.eof()
                    {
                        return Err(error(
                            "ZRYNA-W3004",
                            "invalid private arena, observation or clone-frontier global",
                        ));
                    }
                }
                if globals != expected_globals {
                    return Err(error(
                        "ZRYNA-W3004",
                        "missing private observation or clone-frontier global",
                    ));
                }
            }
            Payload::ImportSection(_)
            | Payload::TableSection(_)
            | Payload::StartSection { .. }
            | Payload::ElementSection(_)
            | Payload::DataSection(_)
            | Payload::TagSection(_)
            | Payload::CustomSection(_)
            | Payload::DataCountSection { .. }
            | Payload::UnknownSection { .. } => {
                return Err(error(
                    "ZRYNA-W3004",
                    "module contains an unapproved capability section",
                ));
            }
            other => {
                return Err(error("ZRYNA-W3004", format!("unapproved module payload: {other:?}")));
            }
        }
    }
    if memories != 1 {
        return Err(error("ZRYNA-W3004", "module is missing bounded linear memory"));
    }
    if globals != expected_globals {
        return Err(error("ZRYNA-W3004", "module is missing its private arena global"));
    }
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|failure| error("ZRYNA-W3002", failure.to_string()))? {
            Payload::ExportSection(exports) => {
                for export in exports {
                    let export =
                        export.map_err(|failure| error("ZRYNA-W3002", failure.to_string()))?;
                    if export.kind != ExternalKind::Func {
                        return Err(error(
                            "ZRYNA-W3004",
                            "only sealed function exports are allowed",
                        ));
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let mut operators = body
                    .get_operators_reader()
                    .map_err(|failure| error("ZRYNA-W3002", failure.to_string()))?;
                while !operators.eof() {
                    let operator = operators
                        .read()
                        .map_err(|failure| error("ZRYNA-W3002", failure.to_string()))?;
                    if !approved_operator(&operator) {
                        return Err(error(
                            "ZRYNA-W3004",
                            format!("unapproved DataOwnershipV1 operator: {operator:?}"),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn private_global_count(program: &VerifiedProgram) -> u32 {
    use zryna_ir::data_ownership_v1::VerifiedInstructionKind as K;
    let prefix = program
        .modules()
        .flat_map(zryna_ir::data_ownership_v1::VerifiedModule::functions)
        .flat_map(VerifiedFunction::blocks)
        .flat_map(zryna_ir::data_ownership_v1::VerifiedBlock::instructions)
        .any(|instruction| match instruction.kind() {
            K::VecClone => instruction.vec_clone_element_failure_drop_actions().next().is_some(),
            K::ClonePlace => {
                instruction.aggregate_clone_element_failure_drop_actions().next().is_some()
            }
            K::GenericClonePlace | K::GenericCloneBorrow => {
                instruction.generic_clone_prefix_failure_drop_actions().next().is_some()
            }
            K::HandleAwareClonePlace | K::HandleAwareCloneBorrow => {
                instruction.handle_aware_clone_prefix_failure_drop_actions().next().is_some()
            }
            _ => false,
        });
    if prefix { 10 } else { 6 }
}

pub(crate) fn approved_operator(operator: &Operator<'_>) -> bool {
    matches!(
        operator,
        Operator::Unreachable
            | Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::If { .. }
            | Operator::Else
            | Operator::End
            | Operator::Br { .. }
            | Operator::BrIf { .. }
            | Operator::Return
            | Operator::Call { .. }
            | Operator::LocalGet { .. }
            | Operator::LocalSet { .. }
            | Operator::LocalTee { .. }
            | Operator::GlobalGet { .. }
            | Operator::GlobalSet { .. }
            | Operator::I32Load { .. }
            | Operator::I32Load8U { .. }
            | Operator::I32Store { .. }
            | Operator::I32Store8 { .. }
            | Operator::I32Const { .. }
            | Operator::I32Eqz
            | Operator::I32Eq
            | Operator::I32Ne
            | Operator::I32LtS
            | Operator::I32LtU
            | Operator::I32GtS
            | Operator::I32GtU
            | Operator::I32LeS
            | Operator::I32GeS
            | Operator::I32GeU
            | Operator::I32Add
            | Operator::I32Sub
            | Operator::I32Mul
            | Operator::I32And
            | Operator::I32ShrU
            | Operator::Drop
            | Operator::I32Shl
    )
}

fn error(code: &'static str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(code, None, message, "report the smallest reproducible verified M3 program")
}
