//! Deterministic core WebAssembly emission from verified Zryna IR.
#![forbid(unsafe_code)]
use std::collections::BTreeMap;
use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
    TypeSection, ValType,
};
use wasmparser::{Encoding, ExternalKind, Operator, Parser, Payload, Validator, WasmFeatures};
use zryna_diagnostics::Diagnostic;
use zryna_ir::control_flow_v1::{
    FunctionIdentity, ValueIdentity, VerifiedFunction as VerifiedControlFlowFunction,
    VerifiedInstructionKind, VerifiedProgram as VerifiedControlFlowProgram, VerifiedTerminatorKind,
};
use zryna_ir::{ExprKind, Type, VerifiedFunction, VerifiedProgram};
mod artifact;
mod scalar_audit;
pub use artifact::ValidatedWebAssemblyArtifact;
use scalar_audit::{audit_profile, seal};
mod data_ownership_v1;
pub use data_ownership_v1::emit_data_ownership;
mod wit_world_audit;
pub use wit_world_audit::{
    ResolvedWitWorld, WitSource, WitWorldAudit, audit_pinned_wit_worlds, pinned_wit_sources,
};
mod component_command;
pub use component_command::{ValidatedCommandComponent, emit_command_self_check};
mod scalar_component;
pub use scalar_component::{ScalarExport, ValidatedScalarComponent, emit_scalar_component};
const MAX_CONTROL_FLOW_WEBASSEMBLY_BYTES: usize = 32 * 1024 * 1024;
/// Emits deterministic, import-free core WebAssembly from the current `I32V1` profile.
/// Raw Universal IR cannot enter this boundary:
///
/// ```compile_fail
/// let raw = zryna_ir::Program::default();
/// let _ = zryna_backend_webassembly::emit(&raw);
/// ```
///
/// The separate raw `ControlFlowV1` profile cannot satisfy this M1 backend boundary either:
///
/// ```compile_fail
/// let raw = zryna_ir::control_flow_v1::raw::Program {
///     entry_module: zryna_ir::control_flow_v1::raw::ModuleId(0),
///     modules: Vec::new(),
/// };
/// let _ = zryna_backend_webassembly::emit(&raw);
/// ```
///
/// # Errors
///
/// Returns a stable compiler diagnostic if a verified-program invariant cannot be encoded or if
/// the exact emitted bytes fail WebAssembly 1.0 validation or the narrower Zryna capability audit.
pub fn emit(program: &VerifiedProgram) -> Result<ValidatedWebAssemblyArtifact, Diagnostic> {
    let functions = program.functions().collect::<Vec<_>>();
    if functions.is_empty() {
        return seal(Module::new().finish());
    }

    let mut types = TypeSection::new();
    let mut type_indices = BTreeMap::<usize, u32>::new();
    let mut next_type_index = 0_u32;
    let mut function_type_indices = Vec::with_capacity(functions.len());
    for function in &functions {
        verify_signature(*function)?;
        let parameter_count = function.parameters().len();
        let type_index = if let Some(index) = type_indices.get(&parameter_count) {
            *index
        } else {
            let parameters = std::iter::repeat_n(ValType::I32, parameter_count);
            types.ty().function(parameters, [ValType::I32]);
            let index = next_type_index;
            next_type_index = next_type_index.checked_add(1).ok_or_else(index_error)?;
            type_indices.insert(parameter_count, index);
            index
        };
        function_type_indices.push(type_index);
    }

    let mut function_section = FunctionSection::new();
    let mut export_section = ExportSection::new();
    let mut code_section = CodeSection::new();
    for (function_index, (function, type_index)) in
        functions.iter().zip(function_type_indices).enumerate()
    {
        function_section.function(type_index);
        let function_index = u32::try_from(function_index).map_err(|_| index_error())?;
        export_section.export(
            function.abi_export().webassembly_name().as_str(),
            ExportKind::Func,
            function_index,
        );
        code_section.function(&encode_function(*function)?);
    }

    let mut module = Module::new();
    module.section(&types);
    module.section(&function_section);
    module.section(&export_section);
    module.section(&code_section);
    seal(module.finish())
}

/// Emits deterministic, capability-minimal core WebAssembly from verified `ControlFlowV1` IR.
///
/// This is an internal M2 backend boundary. It does not activate M2 in the public driver or CLI.
/// Raw control-flow IR cannot enter this function:
///
/// ```compile_fail
/// let raw = zryna_ir::control_flow_v1::raw::Program {
///     entry_module: zryna_ir::control_flow_v1::raw::ModuleId(0),
///     modules: Vec::new(),
/// };
/// let _ = zryna_backend_webassembly::emit_control_flow(&raw);
/// ```
///
/// # Errors
///
/// Returns a stable diagnostic if the sealed profile cannot be represented within the exact
/// artifact budget, validation fails, or the independent capability audit observes drift.
pub fn emit_control_flow(
    program: &VerifiedControlFlowProgram,
) -> Result<ValidatedWebAssemblyArtifact, Diagnostic> {
    emit_control_flow_with_budget(program, MAX_CONTROL_FLOW_WEBASSEMBLY_BYTES)
}

fn emit_control_flow_with_budget(
    program: &VerifiedControlFlowProgram,
    byte_budget: usize,
) -> Result<ValidatedWebAssemblyArtifact, Diagnostic> {
    let layout = ControlFlowLayout::new(program)?;
    let bytes = encode_control_flow(&layout, byte_budget)?;
    Validator::new_with_features(WasmFeatures::WASM1)
        .validate_all(&bytes)
        .map_err(control_flow_validation_error)?;
    audit_control_flow_profile(&bytes, &layout)?;
    Ok(ValidatedWebAssemblyArtifact { bytes })
}

struct ControlFlowLayout<'program> {
    functions: Vec<VerifiedControlFlowFunction<'program>>,
    function_indices: BTreeMap<FunctionIdentity, u32>,
    type_parameter_counts: Vec<usize>,
    function_type_indices: Vec<u32>,
    exports: Vec<(String, u32)>,
    local_counts: Vec<u32>,
}

impl<'program> ControlFlowLayout<'program> {
    fn new(program: &'program VerifiedControlFlowProgram) -> Result<Self, Diagnostic> {
        let functions = program
            .modules()
            .flat_map(zryna_ir::control_flow_v1::VerifiedModule::functions)
            .collect::<Vec<_>>();
        let mut function_indices = BTreeMap::new();
        for (index, function) in functions.iter().enumerate() {
            let index = u32::try_from(index).map_err(|_| control_flow_index_error())?;
            function_indices.insert(function.id(), index);
        }

        let mut type_indices = BTreeMap::<usize, u32>::new();
        let mut type_parameter_counts = Vec::new();
        let mut function_type_indices = Vec::with_capacity(functions.len());
        let mut exports = Vec::new();
        let mut local_counts = Vec::with_capacity(functions.len());
        for (function_index, function) in functions.iter().enumerate() {
            verify_control_flow_signature(*function)?;
            let parameter_count = function.parameters().len();
            let type_index = if let Some(index) = type_indices.get(&parameter_count) {
                *index
            } else {
                let index = u32::try_from(type_parameter_counts.len())
                    .map_err(|_| control_flow_index_error())?;
                type_indices.insert(parameter_count, index);
                type_parameter_counts.push(parameter_count);
                index
            };
            function_type_indices.push(type_index);
            if let Some(public_export) = function.public_export() {
                exports.push((
                    public_export.webassembly_name().as_str().to_owned(),
                    u32::try_from(function_index).map_err(|_| control_flow_index_error())?,
                ));
            }
            local_counts.push(function_local_layout(*function)?.declared_count);
        }
        Ok(Self {
            functions,
            function_indices,
            type_parameter_counts,
            function_type_indices,
            exports,
            local_counts,
        })
    }
}

#[derive(Clone, Copy)]
struct FunctionLocalLayout {
    scratch_base: u32,
    state: u32,
    declared_count: u32,
}

fn function_local_layout(
    function: VerifiedControlFlowFunction<'_>,
) -> Result<FunctionLocalLayout, Diagnostic> {
    let parameter_count = function.parameters().len();
    let blocks = function.blocks().collect::<Vec<_>>();
    let block_value_count = blocks
        .iter()
        .try_fold(0_usize, |count, block| {
            count
                .checked_add(block.parameters().len())
                .and_then(|count| count.checked_add(block.instructions().len()))
        })
        .ok_or_else(control_flow_index_error)?;
    let value_count =
        parameter_count.checked_add(block_value_count).ok_or_else(control_flow_index_error)?;
    let scratch_count = blocks.iter().map(|block| block.parameters().len()).max().unwrap_or(0);
    let declared_count = value_count
        .checked_sub(parameter_count)
        .and_then(|count| count.checked_add(scratch_count))
        .and_then(|count| count.checked_add(1))
        .ok_or_else(control_flow_index_error)?;
    let scratch_base = u32::try_from(value_count).map_err(|_| control_flow_index_error())?;
    let state =
        u32::try_from(value_count.checked_add(scratch_count).ok_or_else(control_flow_index_error)?)
            .map_err(|_| control_flow_index_error())?;
    Ok(FunctionLocalLayout {
        scratch_base,
        state,
        declared_count: u32::try_from(declared_count).map_err(|_| control_flow_index_error())?,
    })
}

fn verify_control_flow_signature(
    function: VerifiedControlFlowFunction<'_>,
) -> Result<(), Diagnostic> {
    if function.parameters().all(|(_, ty, _)| ty != Type::Unit) && function.result() != Type::Unit {
        Ok(())
    } else {
        Err(control_flow_profile_error(function.id()))
    }
}

fn encode_control_flow(
    layout: &ControlFlowLayout<'_>,
    byte_budget: usize,
) -> Result<Vec<u8>, Diagnostic> {
    let mut module = BoundedBytes::new(byte_budget);
    module.extend(b"\0asm\x01\0\0\0")?;
    if layout.functions.is_empty() {
        return Ok(module.finish());
    }

    let mut types = BoundedBytes::new(byte_budget);
    types.u32(
        u32::try_from(layout.type_parameter_counts.len())
            .map_err(|_| control_flow_index_error())?,
    )?;
    for parameter_count in &layout.type_parameter_counts {
        types.byte(0x60)?;
        types.u32(u32::try_from(*parameter_count).map_err(|_| control_flow_index_error())?)?;
        for _ in 0..*parameter_count {
            types.byte(0x7f)?;
        }
        types.extend(&[0x01, 0x7f])?;
    }
    module.section(1, &types.finish())?;

    let mut functions = BoundedBytes::new(byte_budget);
    functions
        .u32(u32::try_from(layout.functions.len()).map_err(|_| control_flow_index_error())?)?;
    for type_index in &layout.function_type_indices {
        functions.u32(*type_index)?;
    }
    module.section(3, &functions.finish())?;

    let mut exports = BoundedBytes::new(byte_budget);
    exports.u32(u32::try_from(layout.exports.len()).map_err(|_| control_flow_index_error())?)?;
    for (name, function_index) in &layout.exports {
        exports.name(name)?;
        exports.byte(0x00)?;
        exports.u32(*function_index)?;
    }
    module.section(7, &exports.finish())?;

    let mut code = BoundedBytes::new(byte_budget);
    code.u32(u32::try_from(layout.functions.len()).map_err(|_| control_flow_index_error())?)?;
    for function in &layout.functions {
        let body = encode_control_flow_function(*function, layout, byte_budget)?;
        code.u32(u32::try_from(body.len()).map_err(|_| control_flow_index_error())?)?;
        code.extend(&body)?;
    }
    module.section(10, &code.finish())?;
    Ok(module.finish())
}

fn encode_control_flow_function(
    function: VerifiedControlFlowFunction<'_>,
    program: &ControlFlowLayout<'_>,
    byte_budget: usize,
) -> Result<Vec<u8>, Diagnostic> {
    let locals = function_local_layout(function)?;
    let mut body = BoundedBytes::new(byte_budget);
    if locals.declared_count == 0 {
        body.byte(0x00)?;
    } else {
        body.byte(0x01)?;
        body.u32(locals.declared_count)?;
        body.byte(0x7f)?;
    }

    if function.public_export().is_some() {
        for (value, ty, _) in function.parameters() {
            if ty == Type::Bool {
                emit_bool_parameter_guard(&mut body, value)?;
            }
        }
    }

    let blocks = function.blocks().collect::<Vec<_>>();
    let block_parameters = blocks
        .iter()
        .map(|block| block.parameters().map(|(value, _, _)| value).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    body.instruction(0x03)?;
    body.byte(0x40)?;
    for block in &blocks {
        body.instruction(0x02)?;
        body.byte(0x40)?;
        body.local_get(locals.state)?;
        body.i32_const(i32::try_from(block.id().index()).map_err(|_| control_flow_index_error())?)?;
        body.instruction(0x47)?;
        body.branch(0x0d, 0)?;
        for instruction in block.instructions() {
            encode_control_flow_instruction(
                instruction.kind(),
                instruction.result(),
                program,
                &mut body,
            )?;
        }
        encode_control_flow_terminator(
            block.terminator().kind(),
            &block_parameters,
            locals,
            &mut body,
        )?;
        body.instruction(0x0b)?;
    }
    body.instruction(0x00)?;
    body.instruction(0x0b)?;
    body.i32_const(0)?;
    body.instruction(0x0b)?;
    Ok(body.finish())
}

fn emit_bool_parameter_guard(
    body: &mut BoundedBytes,
    value: ValueIdentity,
) -> Result<(), Diagnostic> {
    body.local_get(value.index())?;
    body.i32_const(0)?;
    body.instruction(0x46)?;
    body.local_get(value.index())?;
    body.i32_const(1)?;
    body.instruction(0x46)?;
    body.instruction(0x72)?;
    body.instruction(0x04)?;
    body.byte(0x40)?;
    body.instruction(0x05)?;
    body.instruction(0x00)?;
    body.instruction(0x0b)
}

fn encode_control_flow_instruction(
    kind: VerifiedInstructionKind<'_>,
    result: ValueIdentity,
    program: &ControlFlowLayout<'_>,
    body: &mut BoundedBytes,
) -> Result<(), Diagnostic> {
    match kind {
        VerifiedInstructionKind::BoolLiteral(value) => body.i32_const(i32::from(value))?,
        VerifiedInstructionKind::I32Literal(value) => body.i32_const(value)?,
        VerifiedInstructionKind::I32Add(lhs, rhs) => {
            body.local_get(lhs.index())?;
            body.local_get(rhs.index())?;
            body.instruction(0x6a)?;
        }
        VerifiedInstructionKind::I32Sub(lhs, rhs) => {
            body.local_get(lhs.index())?;
            body.local_get(rhs.index())?;
            body.instruction(0x6b)?;
        }
        VerifiedInstructionKind::I32Mul(lhs, rhs) => {
            body.local_get(lhs.index())?;
            body.local_get(rhs.index())?;
            body.instruction(0x6c)?;
        }
        VerifiedInstructionKind::I32Neg(operand) => {
            body.i32_const(0)?;
            body.local_get(operand.index())?;
            body.instruction(0x6b)?;
        }
        VerifiedInstructionKind::Eq(lhs, rhs) => {
            body.local_get(lhs.index())?;
            body.local_get(rhs.index())?;
            body.instruction(0x46)?;
        }
        VerifiedInstructionKind::Ne(lhs, rhs) => {
            body.local_get(lhs.index())?;
            body.local_get(rhs.index())?;
            body.instruction(0x47)?;
        }
        VerifiedInstructionKind::I32LtS(lhs, rhs) => {
            body.local_get(lhs.index())?;
            body.local_get(rhs.index())?;
            body.instruction(0x48)?;
        }
        VerifiedInstructionKind::I32LeS(lhs, rhs) => {
            body.local_get(lhs.index())?;
            body.local_get(rhs.index())?;
            body.instruction(0x4c)?;
        }
        VerifiedInstructionKind::I32GtS(lhs, rhs) => {
            body.local_get(lhs.index())?;
            body.local_get(rhs.index())?;
            body.instruction(0x4a)?;
        }
        VerifiedInstructionKind::I32GeS(lhs, rhs) => {
            body.local_get(lhs.index())?;
            body.local_get(rhs.index())?;
            body.instruction(0x4e)?;
        }
        VerifiedInstructionKind::DirectCall { callee, arguments } => {
            for argument in arguments.iter() {
                body.local_get(argument.index())?;
            }
            let function_index = program
                .function_indices
                .get(&callee)
                .copied()
                .ok_or_else(|| control_flow_profile_error(callee))?;
            body.instruction(0x10)?;
            body.u32(function_index)?;
        }
    }
    body.local_set(result.index())
}

fn encode_control_flow_terminator(
    kind: VerifiedTerminatorKind<'_>,
    block_parameters: &[Vec<ValueIdentity>],
    locals: FunctionLocalLayout,
    body: &mut BoundedBytes,
) -> Result<(), Diagnostic> {
    match kind {
        VerifiedTerminatorKind::Return(value) => {
            body.local_get(value.index())?;
            body.instruction(0x0f)
        }
        VerifiedTerminatorKind::Jump { target, arguments } => {
            encode_control_flow_edge(
                target.index(),
                arguments.iter(),
                block_parameters,
                locals,
                body,
            )?;
            body.branch(0x0c, 1)
        }
        VerifiedTerminatorKind::Branch {
            condition,
            true_target,
            true_arguments,
            false_target,
            false_arguments,
        } => {
            body.local_get(condition.index())?;
            body.instruction(0x04)?;
            body.byte(0x40)?;
            encode_control_flow_edge(
                true_target.index(),
                true_arguments.iter(),
                block_parameters,
                locals,
                body,
            )?;
            body.branch(0x0c, 2)?;
            body.instruction(0x05)?;
            encode_control_flow_edge(
                false_target.index(),
                false_arguments.iter(),
                block_parameters,
                locals,
                body,
            )?;
            body.branch(0x0c, 2)?;
            body.instruction(0x0b)
        }
    }
}

fn encode_control_flow_edge(
    target: u32,
    arguments: impl ExactSizeIterator<Item = ValueIdentity>,
    block_parameters: &[Vec<ValueIdentity>],
    locals: FunctionLocalLayout,
    body: &mut BoundedBytes,
) -> Result<(), Diagnostic> {
    for (index, argument) in arguments.enumerate() {
        body.local_get(argument.index())?;
        body.local_set(
            locals
                .scratch_base
                .checked_add(u32::try_from(index).map_err(|_| control_flow_index_error())?)
                .ok_or_else(control_flow_index_error)?,
        )?;
    }
    for (index, parameter) in block_parameters[target as usize].iter().enumerate() {
        body.local_get(
            locals
                .scratch_base
                .checked_add(u32::try_from(index).map_err(|_| control_flow_index_error())?)
                .ok_or_else(control_flow_index_error)?,
        )?;
        body.local_set(parameter.index())?;
    }
    body.i32_const(i32::try_from(target).map_err(|_| control_flow_index_error())?)?;
    body.local_set(locals.state)
}

struct BoundedBytes {
    bytes: Vec<u8>,
    limit: usize,
}

impl BoundedBytes {
    const fn new(limit: usize) -> Self {
        Self { bytes: Vec::new(), limit }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn byte(&mut self, byte: u8) -> Result<(), Diagnostic> {
        self.extend(&[byte])
    }

    fn instruction(&mut self, opcode: u8) -> Result<(), Diagnostic> {
        self.byte(opcode)
    }

    fn extend(&mut self, bytes: &[u8]) -> Result<(), Diagnostic> {
        let length = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(|| control_flow_budget_error(self.limit))?;
        if length > self.limit {
            return Err(control_flow_budget_error(self.limit));
        }
        self.bytes.try_reserve(bytes.len()).map_err(|_| control_flow_budget_error(self.limit))?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn section(&mut self, id: u8, payload: &[u8]) -> Result<(), Diagnostic> {
        self.byte(id)?;
        self.u32(u32::try_from(payload.len()).map_err(|_| control_flow_index_error())?)?;
        self.extend(payload)
    }

    fn name(&mut self, name: &str) -> Result<(), Diagnostic> {
        self.u32(u32::try_from(name.len()).map_err(|_| control_flow_index_error())?)?;
        self.extend(name.as_bytes())
    }

    fn local_get(&mut self, index: u32) -> Result<(), Diagnostic> {
        self.instruction(0x20)?;
        self.u32(index)
    }

    fn local_set(&mut self, index: u32) -> Result<(), Diagnostic> {
        self.instruction(0x21)?;
        self.u32(index)
    }

    fn branch(&mut self, opcode: u8, depth: u32) -> Result<(), Diagnostic> {
        self.instruction(opcode)?;
        self.u32(depth)
    }

    fn u32(&mut self, mut value: u32) -> Result<(), Diagnostic> {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            self.byte(byte)?;
            if value == 0 {
                return Ok(());
            }
        }
    }

    fn i32_const(&mut self, value: i32) -> Result<(), Diagnostic> {
        self.instruction(0x41)?;
        self.i32(value)
    }

    fn i32(&mut self, mut value: i32) -> Result<(), Diagnostic> {
        loop {
            let mut byte = value.to_le_bytes()[0] & 0x7f;
            value >>= 7;
            let sign = byte & 0x40 != 0;
            let done = (value == 0 && !sign) || (value == -1 && sign);
            if !done {
                byte |= 0x80;
            }
            self.byte(byte)?;
            if done {
                return Ok(());
            }
        }
    }
}

fn verify_signature(function: VerifiedFunction<'_>) -> Result<(), Diagnostic> {
    let signature_is_i32 = function.parameters().iter().all(|ty| *ty == Type::I32)
        && function.return_type() == Type::I32;
    if signature_is_i32 { Ok(()) } else { Err(profile_invariant_error(function)) }
}

fn encode_function(function: VerifiedFunction<'_>) -> Result<Function, Diagnostic> {
    let mut body = Function::new(Vec::new());
    for expression in function.expressions() {
        match expression.kind {
            ExprKind::Parameter(index) => {
                body.instruction(&Instruction::LocalGet(index));
            }
            ExprKind::I32Literal(value) => {
                body.instruction(&Instruction::I32Const(value));
            }
            ExprKind::I32Add { .. } => {
                body.instruction(&Instruction::I32Add);
            }
            ExprKind::BoolLiteral(_) => return Err(profile_invariant_error(function)),
        }
    }
    body.instruction(&Instruction::End);
    Ok(body)
}

#[allow(clippy::too_many_lines)]
fn audit_control_flow_profile(
    bytes: &[u8],
    expected: &ControlFlowLayout<'_>,
) -> Result<(), Diagnostic> {
    let mut saw_version = false;
    let mut saw_types = false;
    let mut saw_functions = false;
    let mut saw_exports = false;
    let mut saw_code_start = false;
    let mut code_index = 0_usize;
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(control_flow_validation_error)?;
        match payload {
            Payload::Version { encoding: Encoding::Module, .. } if !saw_version => {
                saw_version = true;
            }
            Payload::TypeSection(types) if !saw_types => {
                saw_types = true;
                let mut actual = Vec::new();
                for function_type in types.into_iter_err_on_gc_types() {
                    let function_type = function_type.map_err(control_flow_validation_error)?;
                    if !function_type.params().iter().all(|ty| *ty == wasmparser::ValType::I32)
                        || function_type.results() != [wasmparser::ValType::I32]
                    {
                        return Err(control_flow_audit_error("a non-scalar function type"));
                    }
                    actual.push(function_type.params().len());
                }
                if actual != expected.type_parameter_counts {
                    return Err(control_flow_audit_error("a noncanonical type index inventory"));
                }
            }
            Payload::FunctionSection(functions) if !saw_functions => {
                saw_functions = true;
                let actual = functions
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(control_flow_validation_error)?;
                if actual != expected.function_type_indices {
                    return Err(control_flow_audit_error(
                        "a noncanonical function index inventory",
                    ));
                }
            }
            Payload::ExportSection(exports) if !saw_exports => {
                saw_exports = true;
                let mut actual = Vec::new();
                for export in exports {
                    let export = export.map_err(control_flow_validation_error)?;
                    if export.kind != ExternalKind::Func {
                        return Err(control_flow_audit_error("a non-function export"));
                    }
                    actual.push((export.name.to_owned(), export.index));
                }
                if actual != expected.exports {
                    return Err(control_flow_audit_error(
                        "an export outside sealed public authority",
                    ));
                }
            }
            Payload::CodeSectionStart { count, .. } if !saw_code_start => {
                saw_code_start = true;
                if usize::try_from(count).ok() != Some(expected.functions.len()) {
                    return Err(control_flow_audit_error("a mismatched code body inventory"));
                }
            }
            Payload::CodeSectionEntry(body) => {
                let expected_locals = expected
                    .local_counts
                    .get(code_index)
                    .copied()
                    .ok_or_else(|| control_flow_audit_error("an extra code body"))?;
                code_index += 1;
                let mut actual_locals = 0_u32;
                for local in body.get_locals_reader().map_err(control_flow_validation_error)? {
                    let (count, ty) = local.map_err(control_flow_validation_error)?;
                    if ty != wasmparser::ValType::I32 {
                        return Err(control_flow_audit_error("a non-i32 local declaration"));
                    }
                    actual_locals = actual_locals.checked_add(count).ok_or_else(|| {
                        control_flow_audit_error("an overflowing local inventory")
                    })?;
                }
                if actual_locals != expected_locals {
                    return Err(control_flow_audit_error("a noncanonical local inventory"));
                }
                let mut operators =
                    body.get_operators_reader().map_err(control_flow_validation_error)?;
                while !operators.eof() {
                    match operators.read().map_err(control_flow_validation_error)? {
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
                        | Operator::I32Const { .. }
                        | Operator::I32Eq
                        | Operator::I32Ne
                        | Operator::I32LtS
                        | Operator::I32GtS
                        | Operator::I32LeS
                        | Operator::I32GeS
                        | Operator::I32Add
                        | Operator::I32Sub
                        | Operator::I32Mul
                        | Operator::I32Or => {}
                        _ => {
                            return Err(control_flow_audit_error(
                                "an instruction outside the sealed M2 profile",
                            ));
                        }
                    }
                }
            }
            Payload::End(_) => {}
            _ => {
                return Err(control_flow_audit_error(
                    "a section outside the capability-minimal M2 profile",
                ));
            }
        }
    }
    if !saw_version {
        return Err(control_flow_audit_error("a missing core-module header"));
    }
    if expected.functions.is_empty() {
        if saw_types || saw_functions || saw_exports || saw_code_start || code_index != 0 {
            return Err(control_flow_audit_error("sections in an empty module"));
        }
    } else if !saw_types
        || !saw_functions
        || !saw_exports
        || !saw_code_start
        || code_index != expected.functions.len()
    {
        return Err(control_flow_audit_error("a missing canonical M2 section or code body"));
    }
    Ok(())
}

fn profile_invariant_error(function: VerifiedFunction<'_>) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W1001",
        None,
        format!(
            "verified function '{}' contains a type or operation outside the WebAssembly I32V1 proof profile",
            function.abi_export().webassembly_name().as_str()
        ),
        "report this compiler invariant failure with the smallest reproducible Zryna source",
    )
}

fn index_error() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W1002",
        None,
        "verified WebAssembly indexes exceeded the core binary index space",
        "report this compiler invariant failure with the smallest reproducible Zryna source",
    )
}

fn control_flow_profile_error(function: FunctionIdentity) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W2001",
        None,
        format!(
            "verified function {}:{} contains a type or identity outside the WebAssembly M2 proof profile",
            function.module().index(),
            function.declaration()
        ),
        "report this compiler invariant failure with the smallest reproducible Zryna source",
    )
}

fn control_flow_index_error() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W2002",
        None,
        "verified M2 WebAssembly indexes exceeded the core binary index space",
        "report this compiler invariant failure with the smallest reproducible Zryna source",
    )
}

fn control_flow_validation_error(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W2003",
        None,
        format!("emitted M2 core WebAssembly failed pinned WebAssembly 1.0 validation: {error}"),
        "report this compiler failure with the smallest reproducible Zryna source",
    )
}

fn control_flow_audit_error(observation: &str) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W2004",
        None,
        format!("M2 core WebAssembly contains {observation}"),
        "emit only the exact sealed type, function, export, code, local, and instruction inventory",
    )
}

fn control_flow_budget_error(limit: usize) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W2005",
        None,
        format!("deterministic M2 core WebAssembly exceeds its {limit} byte emission budget"),
        "reduce the verified ControlFlowV1 program below the WebAssembly artifact budget",
    )
}

#[cfg(test)]
mod tests {
    use wasmparser::{ExternalKind, Operator, Parser, Payload, Validator, WasmFeatures};
    use zryna_ir::control_flow_v1::{self, raw as control_flow_raw};
    use zryna_ir::{
        Expr, ExprId, ExprKind, Function, MAX_IR_EXPRESSION_DEPTH, Program, Type, verify,
    };
    use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap, Span};

    fn fixture() -> (SourceMap, Span) {
        let sources = SourceMap::build(vec![SourceFileInput {
            path: "src/module.zry".to_owned(),
            text: "x".to_owned(),
        }])
        .expect("fixture source map must be valid");
        let path = NormalizedSourcePath::new("src/module.zry").expect("fixture path");
        let file = sources.file_id(&path).expect("fixture file");
        let span = sources.span(file, 0, 1).expect("fixture span");
        (sources, span)
    }

    fn control_flow_value(id: u32, ty: Type, span: Span) -> control_flow_raw::ValueDefinition {
        control_flow_raw::ValueDefinition { id: control_flow_raw::ValueId(id), ty, span }
    }

    fn control_flow_instruction(
        id: u32,
        ty: Type,
        span: Span,
        kind: control_flow_raw::InstructionKind,
    ) -> control_flow_raw::Instruction {
        control_flow_raw::Instruction { result: control_flow_value(id, ty, span), kind }
    }

    fn control_flow_terminator(
        span: Span,
        kind: control_flow_raw::Terminator,
    ) -> Vec<control_flow_raw::SpannedTerminator> {
        vec![control_flow_raw::SpannedTerminator { span, kind }]
    }

    #[allow(clippy::too_many_lines)]
    fn control_flow_fixture() -> (control_flow_v1::VerifiedProgram, SourceMap) {
        use control_flow_raw::{
            Block, BlockId, Function as ControlFlowFunction, FunctionId, InstructionKind as I,
            Module, ModuleId, Terminator as T, ValueId,
        };

        let sources = SourceMap::build(vec![SourceFileInput {
            path: "src/main.zry".to_owned(),
            text: "x".to_owned(),
        }])
        .expect("control-flow fixture source map");
        let path = NormalizedSourcePath::new("src/main.zry").expect("fixture path");
        let file = sources.file_id(&path).expect("fixture file");
        let span = sources.span(file, 0, 1).expect("fixture span");
        let binary = ControlFlowFunction {
            id: FunctionId { module: ModuleId(0), declaration: 0 },
            entry_export: None,
            span,
            parameters: vec![
                control_flow_value(0, Type::I32, span),
                control_flow_value(1, Type::I32, span),
            ],
            result: Type::I32,
            blocks: vec![Block {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![control_flow_instruction(
                    2,
                    Type::I32,
                    span,
                    I::I32Add { lhs: ValueId(0), rhs: ValueId(1) },
                )],
                terminators: control_flow_terminator(span, T::Return(ValueId(2))),
            }],
        };
        let operations = ControlFlowFunction {
            id: FunctionId { module: ModuleId(0), declaration: 1 },
            entry_export: Some("Math".to_owned()),
            span,
            parameters: vec![
                control_flow_value(0, Type::Bool, span),
                control_flow_value(1, Type::I32, span),
                control_flow_value(2, Type::I32, span),
            ],
            result: Type::I32,
            blocks: vec![
                Block {
                    id: BlockId(0),
                    parameters: Vec::new(),
                    instructions: vec![
                        control_flow_instruction(3, Type::Bool, span, I::BoolLiteral(true)),
                        control_flow_instruction(4, Type::I32, span, I::I32Literal(i32::MIN)),
                        control_flow_instruction(
                            5,
                            Type::I32,
                            span,
                            I::I32Add { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            6,
                            Type::I32,
                            span,
                            I::I32Sub { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            7,
                            Type::I32,
                            span,
                            I::I32Mul { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            8,
                            Type::I32,
                            span,
                            I::I32Neg { operand: ValueId(4) },
                        ),
                        control_flow_instruction(
                            9,
                            Type::Bool,
                            span,
                            I::Eq { lhs: ValueId(0), rhs: ValueId(3) },
                        ),
                        control_flow_instruction(
                            10,
                            Type::Bool,
                            span,
                            I::Ne { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            11,
                            Type::Bool,
                            span,
                            I::I32LtS { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            12,
                            Type::Bool,
                            span,
                            I::I32LeS { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            13,
                            Type::Bool,
                            span,
                            I::I32GtS { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            14,
                            Type::Bool,
                            span,
                            I::I32GeS { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            15,
                            Type::I32,
                            span,
                            I::DirectCall {
                                callee: FunctionId { module: ModuleId(0), declaration: 0 },
                                arguments: vec![ValueId(1), ValueId(2)],
                            },
                        ),
                    ],
                    terminators: control_flow_terminator(
                        span,
                        T::Branch {
                            condition: ValueId(0),
                            true_target: BlockId(1),
                            true_arguments: vec![ValueId(7)],
                            false_target: BlockId(1),
                            false_arguments: vec![ValueId(15)],
                        },
                    ),
                },
                Block {
                    id: BlockId(1),
                    parameters: vec![control_flow_value(16, Type::I32, span)],
                    instructions: Vec::new(),
                    terminators: control_flow_terminator(span, T::Return(ValueId(16))),
                },
            ],
        };
        let swap_loop = ControlFlowFunction {
            id: FunctionId { module: ModuleId(0), declaration: 2 },
            entry_export: Some("Object".to_owned()),
            span,
            parameters: vec![
                control_flow_value(0, Type::I32, span),
                control_flow_value(1, Type::I32, span),
                control_flow_value(2, Type::I32, span),
            ],
            result: Type::I32,
            blocks: vec![
                Block {
                    id: BlockId(0),
                    parameters: Vec::new(),
                    instructions: Vec::new(),
                    terminators: control_flow_terminator(
                        span,
                        T::Jump {
                            target: BlockId(1),
                            arguments: vec![ValueId(0), ValueId(1), ValueId(2)],
                        },
                    ),
                },
                Block {
                    id: BlockId(1),
                    parameters: vec![
                        control_flow_value(3, Type::I32, span),
                        control_flow_value(4, Type::I32, span),
                        control_flow_value(5, Type::I32, span),
                    ],
                    instructions: vec![
                        control_flow_instruction(6, Type::I32, span, I::I32Literal(1)),
                        control_flow_instruction(
                            7,
                            Type::Bool,
                            span,
                            I::I32LtS { lhs: ValueId(3), rhs: ValueId(6) },
                        ),
                    ],
                    terminators: control_flow_terminator(
                        span,
                        T::Branch {
                            condition: ValueId(7),
                            true_target: BlockId(2),
                            true_arguments: Vec::new(),
                            false_target: BlockId(3),
                            false_arguments: vec![ValueId(4)],
                        },
                    ),
                },
                Block {
                    id: BlockId(2),
                    parameters: Vec::new(),
                    instructions: vec![
                        control_flow_instruction(8, Type::I32, span, I::I32Literal(1)),
                        control_flow_instruction(
                            9,
                            Type::I32,
                            span,
                            I::I32Add { lhs: ValueId(3), rhs: ValueId(8) },
                        ),
                    ],
                    terminators: control_flow_terminator(
                        span,
                        T::Jump {
                            target: BlockId(1),
                            arguments: vec![ValueId(9), ValueId(5), ValueId(4)],
                        },
                    ),
                },
                Block {
                    id: BlockId(3),
                    parameters: vec![control_flow_value(10, Type::I32, span)],
                    instructions: Vec::new(),
                    terminators: control_flow_terminator(span, T::Return(ValueId(10))),
                },
            ],
        };
        let boolean_identity = ControlFlowFunction {
            id: FunctionId { module: ModuleId(0), declaration: 3 },
            entry_export: Some("Number".to_owned()),
            span,
            parameters: vec![control_flow_value(0, Type::Bool, span)],
            result: Type::Bool,
            blocks: vec![Block {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminators: control_flow_terminator(span, T::Return(ValueId(0))),
            }],
        };
        let program = control_flow_raw::Program {
            entry_module: ModuleId(0),
            modules: vec![Module {
                id: ModuleId(0),
                source_file: file,
                functions: vec![binary, operations, swap_loop, boolean_identity],
            }],
        };
        let verified = control_flow_v1::verify(program, &sources, file)
            .expect("complete ControlFlowV1 fixture must verify");
        (verified, sources)
    }

    fn add_function(span: Span, name: &str) -> Function {
        Function {
            name: name.to_owned(),
            parameters: vec![Type::I32, Type::I32],
            return_type: Type::I32,
            expressions: vec![
                Expr { ty: Type::I32, span, kind: ExprKind::Parameter(0) },
                Expr { ty: Type::I32, span, kind: ExprKind::Parameter(1) },
                Expr {
                    ty: Type::I32,
                    span,
                    kind: ExprKind::I32Add { lhs: ExprId(0), rhs: ExprId(1) },
                },
            ],
            body: ExprId(2),
        }
    }

    #[test]
    fn emits_exact_minimal_add_module() {
        let (sources, span) = fixture();
        let verified = verify(Program { functions: vec![add_function(span, "add")] }, &sources)
            .expect("add fixture must verify");
        let artifact = super::emit(&verified).expect("add fixture must emit");
        let expected = [
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x07, 0x01, 0x60, 0x02, 0x7f,
            0x7f, 0x01, 0x7f, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64,
            0x00, 0x00, 0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b,
        ];
        assert_eq!(artifact.bytes(), expected);
    }

    #[test]
    fn emission_is_byte_deterministic_and_capability_free() {
        let (sources, span) = fixture();
        let verified = verify(
            Program { functions: vec![add_function(span, "add"), add_function(span, "sum")] },
            &sources,
        )
        .expect("fixture must verify");
        let first = super::emit(&verified).expect("fixture must emit");
        let second = super::emit(&verified).expect("repeated fixture must emit");
        assert_eq!(first, second);

        let mut type_count = 0;
        let mut exports = Vec::new();
        for payload in Parser::new(0).parse_all(first.bytes()) {
            match payload.expect("emitted bytes must parse") {
                Payload::TypeSection(reader) => type_count += reader.count(),
                Payload::ExportSection(reader) => {
                    for export in reader {
                        let export = export.expect("export must parse");
                        assert_eq!(export.kind, ExternalKind::Func);
                        exports.push(export.name.to_owned());
                    }
                }
                Payload::ImportSection(_)
                | Payload::MemorySection(_)
                | Payload::TableSection(_)
                | Payload::GlobalSection(_)
                | Payload::StartSection { .. }
                | Payload::ElementSection(_)
                | Payload::DataSection(_) => panic!("emitter introduced a capability section"),
                _ => {}
            }
        }
        assert_eq!(type_count, 1, "equal signatures must use one stable first-use type");
        assert_eq!(exports, ["add", "sum"]);
    }

    #[test]
    fn signed_leb_i32_boundaries_are_valid_and_exact() {
        let (sources, span) = fixture();
        let values = [0, -1, i32::MIN, i32::MAX];
        let functions = values
            .iter()
            .enumerate()
            .map(|(index, value)| Function {
                name: format!("value{index}"),
                parameters: Vec::new(),
                return_type: Type::I32,
                expressions: vec![Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(*value) }],
                body: ExprId(0),
            })
            .collect();
        let verified =
            verify(Program { functions }, &sources).expect("literal fixture must verify");
        let artifact = super::emit(&verified).expect("literal fixture must emit");
        Validator::new_with_features(WasmFeatures::WASM1)
            .validate_all(artifact.bytes())
            .expect("boundary module must validate");
        let mut encoded_values = Vec::new();
        for payload in Parser::new(0).parse_all(artifact.bytes()) {
            if let Payload::CodeSectionEntry(body) = payload.expect("payload must parse") {
                let mut operators = body.get_operators_reader().expect("operators");
                if let Operator::I32Const { value } = operators.read().expect("literal") {
                    encoded_values.push(value);
                }
            }
        }
        assert_eq!(encoded_values, values);
        assert!(artifact.bytes().windows(5).any(|bytes| bytes == [0x80, 0x80, 0x80, 0x80, 0x78]));
        assert!(artifact.bytes().windows(5).any(|bytes| bytes == [0xff, 0xff, 0xff, 0xff, 0x07]));
    }

    #[test]
    fn maximum_depth_emits_iteratively() {
        let (sources, span) = fixture();
        let mut expressions = vec![Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(1) }];
        let mut root = ExprId(0);
        for _ in 1..MAX_IR_EXPRESSION_DEPTH {
            let leaf = ExprId(u32::try_from(expressions.len()).expect("bounded fixture"));
            expressions.push(Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(1) });
            let parent = ExprId(u32::try_from(expressions.len()).expect("bounded fixture"));
            expressions.push(Expr {
                ty: Type::I32,
                span,
                kind: ExprKind::I32Add { lhs: root, rhs: leaf },
            });
            root = parent;
        }
        let verified = verify(
            Program {
                functions: vec![Function {
                    name: "deepValue".to_owned(),
                    parameters: Vec::new(),
                    return_type: Type::I32,
                    expressions,
                    body: root,
                }],
            },
            &sources,
        )
        .expect("maximum-depth fixture must verify");
        let artifact = super::emit(&verified).expect("maximum-depth fixture must emit");
        let maximum_depth =
            usize::try_from(MAX_IR_EXPRESSION_DEPTH).expect("IR depth limit must fit usize");
        assert!(artifact.bytes().len() < maximum_depth * 8);
    }

    #[test]
    fn validator_and_profile_audit_fail_closed() {
        let malformed = b"not wasm";
        assert_eq!(
            super::seal(malformed.to_vec()).expect_err("bad magic must fail").code(),
            "ZRYNA-W1003"
        );

        let mut module = wasm_encoder::Module::new();
        let mut memories = wasm_encoder::MemorySection::new();
        memories.memory(wasm_encoder::MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        module.section(&memories);
        assert_eq!(
            super::seal(module.finish()).expect_err("memory capability must fail").code(),
            "ZRYNA-W1004"
        );

        let mut module = wasm_encoder::Module::new();
        let mut types = wasm_encoder::TypeSection::new();
        types.ty().function([wasm_encoder::ValType::I64], [wasm_encoder::ValType::I32]);
        module.section(&types);
        assert_eq!(
            super::seal(module.finish()).expect_err("i64 type must fail").code(),
            "ZRYNA-W1004"
        );

        let mut module = wasm_encoder::Module::new();
        let mut types = wasm_encoder::TypeSection::new();
        types.ty().function([wasm_encoder::ValType::V128], [wasm_encoder::ValType::I32]);
        module.section(&types);
        assert_eq!(
            super::seal(module.finish())
                .expect_err("post-MVP SIMD type must fail pinned WebAssembly 1.0 validation")
                .code(),
            "ZRYNA-W1003"
        );
    }

    #[test]
    fn emits_every_m2_operation_terminator_and_canonical_inventory() {
        let (verified, _sources) = control_flow_fixture();
        let first = super::emit_control_flow(&verified).expect("M2 WebAssembly must emit");
        let second =
            super::emit_control_flow(&verified).expect("repeated M2 emission must succeed");
        assert_eq!(first, second);

        let mut type_arities = Vec::new();
        let mut function_types = Vec::new();
        let mut exports = Vec::new();
        let mut operators = Vec::new();
        for payload in Parser::new(0).parse_all(first.bytes()) {
            match payload.expect("M2 artifact must parse") {
                Payload::TypeSection(types) => {
                    for ty in types.into_iter_err_on_gc_types() {
                        type_arities.push(ty.expect("function type").params().len());
                    }
                }
                Payload::FunctionSection(functions) => {
                    function_types
                        .extend(functions.into_iter().map(|index| index.expect("type index")));
                }
                Payload::ExportSection(section) => {
                    for export in section {
                        let export = export.expect("public export");
                        exports.push((export.name.to_owned(), export.index));
                    }
                }
                Payload::CodeSectionEntry(body) => {
                    let mut reader = body.get_operators_reader().expect("operators");
                    while !reader.eof() {
                        operators.push(format!("{:?}", reader.read().expect("operator")));
                    }
                }
                Payload::ImportSection(_)
                | Payload::TableSection(_)
                | Payload::MemorySection(_)
                | Payload::GlobalSection(_)
                | Payload::StartSection { .. }
                | Payload::ElementSection(_)
                | Payload::DataSection(_)
                | Payload::CustomSection(_) => panic!("M2 artifact introduced a capability"),
                _ => {}
            }
        }
        assert_eq!(type_arities, [2, 3, 1]);
        assert_eq!(function_types, [0, 1, 1, 2]);
        assert_eq!(
            exports,
            [("Math".to_owned(), 1), ("Object".to_owned(), 2), ("Number".to_owned(), 3),]
        );
        for required in [
            "I32Add",
            "I32Sub",
            "I32Mul",
            "I32Eq",
            "I32Ne",
            "I32LtS",
            "I32LeS",
            "I32GtS",
            "I32GeS",
            "Call",
            "Block",
            "Loop",
            "BrIf",
            "Br",
            "If",
            "Else",
            "Return",
            "LocalGet",
            "LocalSet",
            "Unreachable",
        ] {
            assert!(operators.iter().any(|operator| operator.starts_with(required)), "{required}");
        }
    }

    #[test]
    fn empty_m2_program_is_only_the_core_module_header() {
        use control_flow_raw::{Module, ModuleId};

        let sources = SourceMap::build(vec![SourceFileInput {
            path: "src/empty.zry".to_owned(),
            text: String::new(),
        }])
        .expect("empty M2 source map");
        let path = NormalizedSourcePath::new("src/empty.zry").expect("empty M2 path");
        let file = sources.file_id(&path).expect("empty M2 file");
        let verified = control_flow_v1::verify(
            control_flow_raw::Program {
                entry_module: ModuleId(0),
                modules: vec![Module { id: ModuleId(0), source_file: file, functions: Vec::new() }],
            },
            &sources,
            file,
        )
        .expect("empty M2 program must verify");
        let artifact = super::emit_control_flow(&verified).expect("empty M2 program must emit");
        assert_eq!(artifact.bytes(), b"\0asm\x01\0\0\0");
    }

    #[test]
    fn m2_artifact_budget_accepts_exact_and_rejects_the_first_extra_byte() {
        let (verified, _sources) = control_flow_fixture();
        let artifact = super::emit_control_flow(&verified).expect("bounded artifact");
        let exact = super::emit_control_flow_with_budget(&verified, artifact.bytes().len())
            .expect("exact byte budget must pass");
        assert_eq!(exact, artifact);
        assert_eq!(
            super::emit_control_flow_with_budget(&verified, artifact.bytes().len() - 1)
                .expect_err("first missing byte must fail")
                .code(),
            "ZRYNA-W2005"
        );
    }

    #[test]
    fn maximum_block_inventory_keeps_dispatcher_nesting_constant() {
        use control_flow_raw::{
            Block, BlockId, Function as ControlFlowFunction, FunctionId, Module, ModuleId,
            Terminator as T, ValueId,
        };

        let sources = SourceMap::build(vec![SourceFileInput {
            path: "src/chain.zry".to_owned(),
            text: "x".to_owned(),
        }])
        .expect("chain source map");
        let path = NormalizedSourcePath::new("src/chain.zry").expect("chain path");
        let file = sources.file_id(&path).expect("chain file");
        let span = sources.span(file, 0, 1).expect("chain span");
        let mut blocks = Vec::with_capacity(4_096);
        for id in 0_u32..4_096 {
            let terminator = if id == 4_095 {
                T::Return(ValueId(0))
            } else {
                T::Jump { target: BlockId(id + 1), arguments: Vec::new() }
            };
            blocks.push(Block {
                id: BlockId(id),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminators: control_flow_terminator(span, terminator),
            });
        }
        let program = control_flow_raw::Program {
            entry_module: ModuleId(0),
            modules: vec![Module {
                id: ModuleId(0),
                source_file: file,
                functions: vec![ControlFlowFunction {
                    id: FunctionId { module: ModuleId(0), declaration: 0 },
                    entry_export: Some("Chain".to_owned()),
                    span,
                    parameters: vec![control_flow_value(0, Type::I32, span)],
                    result: Type::I32,
                    blocks,
                }],
            }],
        };
        let verified = control_flow_v1::verify(program, &sources, file)
            .expect("maximum block inventory must verify");
        let artifact = super::emit_control_flow(&verified).expect("maximum blocks must emit");

        let mut depth = 0_u32;
        let mut maximum_depth = 0_u32;
        for payload in Parser::new(0).parse_all(artifact.bytes()) {
            if let Payload::CodeSectionEntry(body) = payload.expect("module payload") {
                let mut operators = body.get_operators_reader().expect("operators");
                while !operators.eof() {
                    match operators.read().expect("operator") {
                        Operator::Block { .. } | Operator::Loop { .. } | Operator::If { .. } => {
                            depth += 1;
                            maximum_depth = maximum_depth.max(depth);
                        }
                        Operator::End if depth > 0 => depth -= 1,
                        _ => {}
                    }
                }
            }
        }
        assert!(maximum_depth <= 2, "dispatcher depth was {maximum_depth}");
    }

    #[test]
    fn cross_module_private_call_uses_one_flattened_index_without_exporting_dependency() {
        use control_flow_raw::{
            Block, BlockId, Function as ControlFlowFunction, FunctionId, InstructionKind as I,
            Module, ModuleId, Terminator as T, ValueId,
        };

        let sources = SourceMap::build(vec![
            SourceFileInput { path: "src/entry.zry".to_owned(), text: "x".to_owned() },
            SourceFileInput { path: "src/private.zry".to_owned(), text: "x".to_owned() },
        ])
        .expect("cross-module source map");
        let entry_path = NormalizedSourcePath::new("src/entry.zry").expect("entry path");
        let private_path = NormalizedSourcePath::new("src/private.zry").expect("private path");
        let entry_file = sources.file_id(&entry_path).expect("entry file");
        let private_file = sources.file_id(&private_path).expect("private file");
        let entry_span = sources.span(entry_file, 0, 1).expect("entry span");
        let private_span = sources.span(private_file, 0, 1).expect("private span");
        let program = control_flow_raw::Program {
            entry_module: ModuleId(0),
            modules: vec![
                Module {
                    id: ModuleId(0),
                    source_file: entry_file,
                    functions: vec![ControlFlowFunction {
                        id: FunctionId { module: ModuleId(0), declaration: 0 },
                        entry_export: Some("Entry".to_owned()),
                        span: entry_span,
                        parameters: vec![control_flow_value(0, Type::I32, entry_span)],
                        result: Type::I32,
                        blocks: vec![Block {
                            id: BlockId(0),
                            parameters: Vec::new(),
                            instructions: vec![control_flow_instruction(
                                1,
                                Type::I32,
                                entry_span,
                                I::DirectCall {
                                    callee: FunctionId { module: ModuleId(1), declaration: 0 },
                                    arguments: vec![ValueId(0)],
                                },
                            )],
                            terminators: control_flow_terminator(entry_span, T::Return(ValueId(1))),
                        }],
                    }],
                },
                Module {
                    id: ModuleId(1),
                    source_file: private_file,
                    functions: vec![ControlFlowFunction {
                        id: FunctionId { module: ModuleId(1), declaration: 0 },
                        entry_export: None,
                        span: private_span,
                        parameters: vec![control_flow_value(0, Type::I32, private_span)],
                        result: Type::I32,
                        blocks: vec![Block {
                            id: BlockId(0),
                            parameters: Vec::new(),
                            instructions: Vec::new(),
                            terminators: control_flow_terminator(
                                private_span,
                                T::Return(ValueId(0)),
                            ),
                        }],
                    }],
                },
            ],
        };
        let verified = control_flow_v1::verify(program, &sources, entry_file)
            .expect("cross-module fixture must verify");
        let artifact = super::emit_control_flow(&verified).expect("cross-module artifact");
        let mut exports = Vec::new();
        let mut calls = Vec::new();
        for payload in Parser::new(0).parse_all(artifact.bytes()) {
            match payload.expect("cross-module payload") {
                Payload::ExportSection(section) => {
                    for export in section {
                        let export = export.expect("entry export");
                        exports.push((export.name.to_owned(), export.index));
                    }
                }
                Payload::CodeSectionEntry(body) => {
                    let mut operators = body.get_operators_reader().expect("operators");
                    while !operators.eof() {
                        if let Operator::Call { function_index } =
                            operators.read().expect("operator")
                        {
                            calls.push(function_index);
                        }
                    }
                }
                _ => {}
            }
        }
        assert_eq!(exports, [("Entry".to_owned(), 0)]);
        assert_eq!(calls, [1]);
    }

    #[test]
    fn m2_audit_rejects_capability_sections_and_unsealed_exports() {
        let (verified, _sources) = control_flow_fixture();
        let layout = super::ControlFlowLayout::new(&verified).expect("sealed layout");

        let mut module = wasm_encoder::Module::new();
        let mut memories = wasm_encoder::MemorySection::new();
        memories.memory(wasm_encoder::MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        module.section(&memories);
        assert_eq!(
            super::audit_control_flow_profile(&module.finish(), &layout)
                .expect_err("memory capability must fail")
                .code(),
            "ZRYNA-W2004"
        );

        let mut forged = super::encode_control_flow(&layout, 32 * 1024 * 1024)
            .expect("canonical fixture must encode");
        let export_name =
            forged.windows(4).position(|window| window == b"Math").expect("Math export");
        forged[export_name..export_name + 4].copy_from_slice(b"Evil");
        assert_eq!(
            super::audit_control_flow_profile(&forged, &layout)
                .expect_err("unsealed export must fail")
                .code(),
            "ZRYNA-W2004"
        );
    }

    #[test]
    fn m2_audit_rejects_every_forbidden_section_family_and_operator() {
        let (verified, _sources) = control_flow_fixture();
        let layout = super::ControlFlowLayout::new(&verified).expect("sealed layout");

        for (section_id, observation) in [
            (0_u8, "custom"),
            (2, "import"),
            (4, "table"),
            (5, "memory"),
            (6, "global"),
            (8, "start"),
            (9, "element"),
            (11, "data"),
            (12, "data-count"),
            (13, "tag"),
        ] {
            let mut bytes = b"\0asm\x01\0\0\0".to_vec();
            bytes.extend_from_slice(&[section_id, 1, 0]);
            let error = super::audit_control_flow_profile(&bytes, &layout).expect_err(observation);
            assert_eq!(error.code(), "ZRYNA-W2004", "{observation} section");
        }

        let mut unsupported =
            super::encode_control_flow(&layout, 32 * 1024 * 1024).expect("canonical fixture");
        let opcode = unsupported.iter().position(|byte| *byte == 0x6a).expect("i32.add opcode");
        unsupported[opcode] = 0x1a;
        assert_eq!(
            super::audit_control_flow_profile(&unsupported, &layout)
                .expect_err("drop must be outside the sealed operator profile")
                .code(),
            "ZRYNA-W2004"
        );
    }

    #[test]
    fn empty_program_is_only_the_core_module_header() {
        let sources = SourceMap::build(Vec::new()).expect("empty source map");
        let verified = verify(Program::default(), &sources).expect("empty program");
        let artifact = super::emit(&verified).expect("empty module");
        assert_eq!(artifact.bytes(), b"\0asm\x01\0\0\0");
    }
}
