//! Native code-generation boundary.

#![forbid(unsafe_code)]

use std::fmt::Write;

use cranelift_codegen::{
    Context,
    ir::{AbiParam, Function, InstBuilder, Signature, UserFuncName, types},
    isa::CallConv,
    settings::{self, Configurable},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use object::{
    BinaryFormat, Endianness, Object, ObjectKind, ObjectSection, ObjectSymbol, SectionFlags,
    SectionKind,
};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::{
    MirType, OperationView, ValueId, VerifiedCallingConvention, VerifiedMirFunction,
    VerifiedMirModule,
};

/// Internal M2 scalar control-flow object emission.
pub mod control_flow_v1;
/// Internal DataOwnershipV1 Linux x86-64 object emission.
pub mod data_ownership_v1;
pub mod generic_copy_v1;
/// The only native object target implemented by the M1 scalar profile.
pub const NATIVE_OBJECT_TARGET: &str = "x86_64-unknown-linux-gnu";
/// Maximum encoded object bytes accepted by the native object audit.
pub const MAX_NATIVE_OBJECT_BYTES: usize = 8 * 1024 * 1024;
const EMPTY_OBJECT_SECTIONS: [(&str, SectionKind, u64); 4] = [
    (".note.GNU-stack", SectionKind::Other, 0),
    (".symtab", SectionKind::Metadata, 0),
    (".strtab", SectionKind::Metadata, 0),
    (".shstrtab", SectionKind::Metadata, 0),
];
const FUNCTION_OBJECT_SECTIONS: [(&str, SectionKind, u64); 5] = [
    (".text", SectionKind::Text, 6),
    (".note.GNU-stack", SectionKind::Other, 0),
    (".symtab", SectionKind::Metadata, 0),
    (".strtab", SectionKind::Metadata, 0),
    (".shstrtab", SectionKind::Metadata, 0),
];

/// Capability proving that an object request selected the exact supported target.
///
/// ```compile_fail
/// let _ = zryna_backend_native::LinuxX8664ObjectTarget { private: () };
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinuxX8664ObjectTarget {
    private: (),
}

/// Selects the exact supported native object target.
///
/// # Errors
///
/// Returns `ZRYNA-N3001` for every empty, malformed, aliased, or unsupported request.
pub fn select_object_target(requested: &str) -> Result<LinuxX8664ObjectTarget, Diagnostic> {
    if requested == NATIVE_OBJECT_TARGET {
        Ok(LinuxX8664ObjectTarget { private: () })
    } else {
        Err(Diagnostic::error(
            "ZRYNA-N3001",
            None,
            "native object target is unsupported",
            format!("use the exact supported target '{NATIVE_OBJECT_TARGET}'"),
        ))
    }
}

/// ELF object bytes that passed the closed native-object audit.
///
/// ```compile_fail
/// let _ = zryna_backend_native::ValidatedNativeObjectArtifact { bytes: Vec::new() };
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedNativeObjectArtifact {
    bytes: Vec<u8>,
}

impl ValidatedNativeObjectArtifact {
    /// Returns the independently audited ELF relocatable bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Emits one deterministic audited Linux x86-64 ELF relocatable object.
///
/// Only verified native MIR and an exact target capability can enter this boundary. The backend
/// never invokes a system compiler, assembler, linker, loader, or generated executable.
///
/// # Errors
///
/// Returns stable target, code-generation, or post-encode audit diagnostics.
pub fn emit_object(
    module: &VerifiedMirModule,
    _target: LinuxX8664ObjectTarget,
) -> Result<ValidatedNativeObjectArtifact, Diagnostic> {
    let mut flags = settings::builder();
    flags.set("opt_level", "none").map_err(codegen_error)?;
    flags.set("is_pic", "false").map_err(codegen_error)?;
    let triple = NATIVE_OBJECT_TARGET.parse::<target_lexicon::Triple>().map_err(codegen_error)?;
    let isa = cranelift_codegen::isa::lookup(triple)
        .map_err(codegen_error)?
        .finish(settings::Flags::new(flags))
        .map_err(codegen_error)?;
    let mut object_builder = ObjectBuilder::new(isa, b"zryna".to_vec(), default_libcall_names())
        .map_err(codegen_error)?;
    object_builder.per_function_section(false);
    let mut object_module = ObjectModule::new(object_builder);
    let mut function_builder_context = FunctionBuilderContext::new();

    for (function_index, function) in module.functions().enumerate() {
        let frontend_config = object_module.target_config();
        let signature = signature(function)?;
        let function_id = object_module
            .declare_function(function.symbol(), Linkage::Export, &signature)
            .map_err(codegen_error)?;
        let user_index = u32::try_from(function_index).map_err(codegen_error)?;
        let mut context = Context::for_function(Function::with_name_signature(
            UserFuncName::user(0, user_index),
            signature,
        ));
        build_function(function, &mut context, &mut function_builder_context, frontend_config)?;
        object_module.define_function(function_id, &mut context).map_err(codegen_error)?;
    }

    let bytes = object_module.finish().emit().map_err(codegen_error)?;
    audit_object(&bytes, module)?;
    Ok(ValidatedNativeObjectArtifact { bytes })
}

fn signature(function: VerifiedMirFunction<'_>) -> Result<Signature, Diagnostic> {
    match function.calling_convention() {
        VerifiedCallingConvention::ScalarAbiV1LinuxX8664SystemV => {}
    }
    verify_codegen_type(function.result_type())?;
    let mut signature = Signature::new(CallConv::SystemV);
    for ty in function.parameter_types() {
        verify_codegen_type(*ty)?;
        signature.params.push(AbiParam::new(types::I32));
    }
    signature.returns.push(AbiParam::new(types::I32));
    Ok(signature)
}

fn build_function(
    function: VerifiedMirFunction<'_>,
    context: &mut Context,
    builder_context: &mut FunctionBuilderContext,
    frontend_config: cranelift_codegen::isa::TargetFrontendConfig,
) -> Result<(), Diagnostic> {
    let mut builder = FunctionBuilder::new(&mut context.func, builder_context);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    let parameters = builder.block_params(entry).to_vec();
    let mut values = Vec::with_capacity(function.values().len());
    for value in function.values() {
        verify_codegen_type(value.ty())?;
        let encoded = match value.operation() {
            OperationView::Parameter { index } => parameters
                .get(usize::try_from(index).map_err(codegen_error)?)
                .copied()
                .ok_or_else(native_invariant_error)?,
            OperationView::I32Literal { value } => {
                builder.ins().iconst(types::I32, i64::from(value))
            }
            OperationView::I32Add { lhs, rhs } => {
                let lhs = encoded_value(&values, lhs)?;
                let rhs = encoded_value(&values, rhs)?;
                builder.ins().iadd(lhs, rhs)
            }
        };
        values.push(encoded);
    }
    let result = encoded_value(&values, function.result())?;
    builder.ins().return_(&[result]);
    builder.finalize(frontend_config);
    Ok(())
}

fn encoded_value(
    values: &[cranelift_codegen::ir::Value],
    id: ValueId,
) -> Result<cranelift_codegen::ir::Value, Diagnostic> {
    values
        .get(usize::try_from(id.index()).map_err(codegen_error)?)
        .copied()
        .ok_or_else(native_invariant_error)
}

fn audit_object(bytes: &[u8], module: &VerifiedMirModule) -> Result<(), Diagnostic> {
    if bytes.len() > MAX_NATIVE_OBJECT_BYTES {
        return Err(object_audit_error());
    }
    let file = object::File::parse(bytes).map_err(|_| object_audit_error())?;
    if file.format() != BinaryFormat::Elf
        || file.architecture() != object::Architecture::X86_64
        || file.endianness() != Endianness::Little
        || file.kind() != ObjectKind::Relocatable
        || !file.is_64()
    {
        return Err(object_audit_error());
    }
    let expected_sections = if module.functions().len() == 0 {
        &EMPTY_OBJECT_SECTIONS[..]
    } else {
        &FUNCTION_OBJECT_SECTIONS[..]
    };
    let sections = file.sections().collect::<Vec<_>>();
    if sections.len() != expected_sections.len() {
        return Err(object_audit_error());
    }
    for (section, (expected_name, expected_kind, expected_flags)) in
        sections.into_iter().zip(expected_sections.iter().copied())
    {
        let SectionFlags::Elf { sh_flags } = section.flags() else {
            return Err(object_audit_error());
        };
        if section.name().map_err(|_| object_audit_error())? != expected_name
            || section.kind() != expected_kind
            || sh_flags != expected_flags
            || section.relocations().next().is_some()
        {
            return Err(object_audit_error());
        }
    }
    let expected = module.functions().map(VerifiedMirFunction::symbol).collect::<Vec<_>>();
    let mut observed = Vec::new();
    for symbol in file.symbols() {
        if symbol.is_undefined() {
            return Err(object_audit_error());
        }
        if symbol.is_global() {
            if symbol.kind() != object::SymbolKind::Text {
                return Err(object_audit_error());
            }
            let name = symbol.name().map_err(|_| object_audit_error())?;
            if symbol.size() == 0 {
                return Err(object_audit_error());
            }
            observed.push(name);
        }
    }
    if observed != expected {
        return Err(object_audit_error());
    }
    Ok(())
}

fn native_invariant_error() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N3002",
        None,
        "native object code generation rejected a verified MIR invariant",
        "report this compiler invariant failure with the smallest reproducible source",
    )
}

fn codegen_error(error: impl std::fmt::Display) -> Diagnostic {
    let _ = error;
    Diagnostic::error(
        "ZRYNA-N3002",
        None,
        "native object code generation failed",
        "report this compiler failure with the smallest reproducible source",
    )
}

fn object_audit_error() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N3003",
        None,
        "native object failed the closed Linux x86-64 ELF audit",
        "report this compiler failure with the smallest reproducible source",
    )
}

/// Textual LLVM IR artifact used to validate the initial native boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LlvmIrArtifact {
    /// LLVM IR module text.
    pub source: String,
}

/// Emits the supported verified MIR slice as LLVM IR text.
///
/// This compatibility proof is independent of the implemented Cranelift object path.
/// Raw MIR is not accepted by this boundary:
///
/// ```compile_fail
/// let raw = zryna_native_mir::raw::Module::new(Vec::new());
/// let _ = zryna_backend_native::emit_llvm_ir(&raw);
/// ```
///
/// # Errors
///
/// Returns a compiler diagnostic when an internal verified-MIR invariant or formatting fails.
pub fn emit_llvm_ir(module: &VerifiedMirModule) -> Result<LlvmIrArtifact, Diagnostic> {
    let mut output = String::new();
    for function in module.functions() {
        emit_function(function, &mut output)?;
    }
    Ok(LlvmIrArtifact { source: output })
}

fn emit_function(function: VerifiedMirFunction<'_>, output: &mut String) -> Result<(), Diagnostic> {
    match function.calling_convention() {
        VerifiedCallingConvention::ScalarAbiV1LinuxX8664SystemV => {}
    }
    verify_codegen_type(function.result_type())?;
    for ty in function.parameter_types() {
        verify_codegen_type(*ty)?;
    }
    write!(output, "define i32 @{}(", function.symbol()).map_err(native_format_error)?;
    for index in 0..function.parameter_types().len() {
        if index > 0 {
            output.push_str(", ");
        }
        write!(output, "i32 %p{index}").map_err(native_format_error)?;
    }
    output.push_str(") {\nentry:\n");
    for value in function.values() {
        verify_codegen_type(value.ty())?;
        let id = value.id().index();
        match value.operation() {
            OperationView::Parameter { .. } => {}
            OperationView::I32Literal { value } => {
                writeln!(output, "  %v{id} = add i32 0, {value}").map_err(native_format_error)?;
            }
            OperationView::I32Add { lhs, rhs } => {
                let left = llvm_value(function, lhs)?;
                let right = llvm_value(function, rhs)?;
                writeln!(output, "  %v{id} = add i32 {left}, {right}")
                    .map_err(native_format_error)?;
            }
        }
    }
    let result = llvm_value(function, function.result())?;
    write!(output, "  ret i32 {result}\n}}\n").map_err(native_format_error)?;
    Ok(())
}

fn llvm_value(function: VerifiedMirFunction<'_>, id: ValueId) -> Result<String, Diagnostic> {
    let value = function.value(id).ok_or_else(|| {
        Diagnostic::error(
            "ZRYNA-N2002",
            None,
            format!("verified native function '{}' references a missing value", function.symbol()),
            "report this compiler invariant failure with the smallest reproducible source",
        )
    })?;
    match value.operation() {
        OperationView::Parameter { index } => Ok(format!("%p{index}")),
        OperationView::I32Literal { .. } | OperationView::I32Add { .. } => {
            Ok(format!("%v{}", id.index()))
        }
    }
}

fn verify_codegen_type(ty: MirType) -> Result<(), Diagnostic> {
    match ty {
        MirType::I32 => Ok(()),
        MirType::Unit | MirType::Bool => Err(Diagnostic::error(
            "ZRYNA-N2001",
            None,
            "verified native MIR contains a type outside the LLVM proof profile",
            "report this compiler invariant failure with the smallest reproducible source",
        )),
    }
}

fn native_format_error(error: std::fmt::Error) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N2003",
        None,
        format!("native IR formatting failed: {error}"),
        "report this compiler failure with the smallest reproducible Zryna source",
    )
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
