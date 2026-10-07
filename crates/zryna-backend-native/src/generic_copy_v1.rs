//! Import-free Linux x86-64 execution of separately sealed generic Copy MIR.

mod audit;
mod body;
mod control;
mod wrappers;

use crate::{LinuxX8664ObjectTarget, NATIVE_OBJECT_TARGET, ValidatedNativeObjectArtifact};
use cranelift_codegen::{
    Context,
    ir::{AbiParam, Function, Signature, UserFuncName, types},
    isa::CallConv,
    settings::{self, Configurable},
};
use cranelift_frontend::FunctionBuilderContext;
use cranelift_module::{Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::generic_copy_v1::VerifiedProgram;

/// Emits audited ELF with private stack result transport and exact scalar ABI wrappers.
///
/// ```compile_fail
/// fn fake(raw: zryna_ir::generic_v1::raw::Program, target: zryna_backend_native::LinuxX8664ObjectTarget) {
///     let _ = zryna_backend_native::generic_copy_v1::emit_object(&raw, target);
/// }
/// ```
/// # Errors
/// Returns a stable invariant/code generation diagnostic or final closed object audit rejection.
pub fn emit_object(
    program: &VerifiedProgram<'_, '_>,
    _target: LinuxX8664ObjectTarget,
) -> Result<ValidatedNativeObjectArtifact, Diagnostic> {
    let mut flags = settings::builder();
    for (key, value) in [("opt_level", "none"), ("is_pic", "false"), ("unwind_info", "false")] {
        flags.set(key, value).map_err(codegen)?;
    }
    let triple = NATIVE_OBJECT_TARGET.parse::<target_lexicon::Triple>().map_err(codegen)?;
    let isa = cranelift_codegen::isa::lookup(triple)
        .map_err(codegen)?
        .finish(settings::Flags::new(flags))
        .map_err(codegen)?;
    let mut builder = ObjectBuilder::new(isa, b"zryna-gcopy-v1".to_vec(), default_libcall_names())
        .map_err(codegen)?;
    builder.per_function_section(false);
    let mut object = ObjectModule::new(builder);
    let mut ids = Vec::new();
    for plan in program.functions() {
        ids.push(
            object
                .declare_function(plan.symbol(), Linkage::Local, &signature(plan.parameters()))
                .map_err(codegen)?,
        );
    }
    let mut wrapper_ids = Vec::new();
    for export in program.program().scalar_abi().exports() {
        wrapper_ids.push(
            object
                .declare_function(
                    export.native_linux_x86_64_symbol().as_str(),
                    Linkage::Export,
                    &wrappers::signature(export),
                )
                .map_err(codegen)?,
        );
    }
    let mut builder_context = FunctionBuilderContext::new();
    for (index, plan) in program.functions().iter().enumerate() {
        let mut context = context(index, signature(plan.parameters()))?;
        let mut callees = std::collections::BTreeMap::new();
        for target in body::callees(program, index)? {
            callees.insert(target, object.declare_func_in_func(ids[target], &mut context.func));
        }
        body::build(
            program,
            index,
            &mut context,
            &mut builder_context,
            &callees,
            object.target_config(),
        )?;
        object.define_function(ids[index], &mut context).map_err(codegen)?;
    }
    for (index, export) in program.program().scalar_abi().exports().enumerate() {
        let target = program.program().export_functions()[index];
        let mut context = context(program.functions().len() + index, wrappers::signature(export))?;
        let callee = object.declare_func_in_func(ids[target], &mut context.func);
        wrappers::build(export, callee, &mut context, &mut builder_context, object.target_config());
        object.define_function(wrapper_ids[index], &mut context).map_err(codegen)?;
    }
    let bytes = object.finish().emit().map_err(codegen)?;
    audit::check(&bytes, program)?;
    Ok(ValidatedNativeObjectArtifact { bytes })
}

fn signature(parameters: u32) -> Signature {
    let mut s = Signature::new(CallConv::SystemV);
    s.params.push(AbiParam::new(types::I64));
    for _ in 0..parameters {
        s.params.push(AbiParam::new(types::I32));
    }
    s
}

/// Checks the closed ELF inventory and direct-call bindings against exact sealed generic MIR.
///
/// This diagnostic utility grants no executable artifact authority and does not authenticate
/// arbitrary machine instructions. Only emission constructs the opaque validated artifact.
/// # Errors
/// Rejects target, section, symbol, extent, relocation and direct-call instruction drift.
pub fn validate_object_inventory(
    bytes: &[u8],
    program: &VerifiedProgram<'_, '_>,
) -> Result<(), Diagnostic> {
    audit::check(bytes, program)
}

fn context(index: usize, s: Signature) -> Result<Context, Diagnostic> {
    Ok(Context::for_function(Function::with_name_signature(
        UserFuncName::user(7, u32::try_from(index).map_err(codegen)?),
        s,
    )))
}
fn codegen(error: impl std::fmt::Display) -> Diagnostic {
    let _ = error;
    Diagnostic::error(
        "ZRYNA-N7102",
        None,
        "generic native code generation rejected a sealed invariant",
        "report the smallest reproducible source",
    )
}
fn invariant() -> Diagnostic {
    codegen("sealed invariant")
}
