//! Audited total-scalar C exports from the genuine independently verified native C machine seal.
//!
//! This explicit export artifact contains every public C export and no private entry or import.
//! It is not an executable implementation of the program's foreign wrappers or dispatcher.

mod audit;
mod scalar;

use cranelift_codegen::{
    Context,
    ir::{Function, UserFuncName},
    settings::{self, Configurable},
};
use cranelift_frontend::FunctionBuilderContext;
use cranelift_module::{Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::VerifiedMirProgram;

use crate::{LinuxX8664ObjectTarget, NATIVE_OBJECT_TARGET};

/// Exact total-scalar export object and generated C header, retaining the original machine seal.
///
/// ```compile_fail
/// let _ = zryna_backend_native::native_c_v0::ValidatedScalarExports {
///     bytes: vec![], header: String::new(), program: todo!()
/// };
/// ```
#[derive(Clone, Debug)]
pub struct ValidatedScalarExports {
    bytes: Vec<u8>,
    header: String,
    program: VerifiedMirProgram,
}
impl ValidatedScalarExports {
    /// Independently audited Linux x86-64 ELF relocatable bytes; no process or link authority.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Exact scalar declarations derived from the retained ABI, including C-int spelling.
    #[must_use]
    pub fn header(&self) -> &str {
        &self.header
    }
    /// Complete original machine authority, including declarations absent from this export object.
    #[must_use]
    pub const fn program(&self) -> &VerifiedMirProgram {
        &self.program
    }
}

/// Emits every admitted total-scalar public C export, with an exact generated header.
///
/// Imports, resource-bearing functions and compiler-private channels are deliberately not emitted
/// by this separate artifact kind. No source function is substituted by a C implementation.
/// # Errors
/// Returns a stable diagnostic for a violated sealed invariant, code generation or ELF audit.
pub fn emit_scalar_exports(
    program: &VerifiedMirProgram,
    _target: LinuxX8664ObjectTarget,
) -> Result<ValidatedScalarExports, Diagnostic> {
    let mut flags = settings::builder();
    flags.set("opt_level", "none").map_err(codegen_error)?;
    flags.set("is_pic", "false").map_err(codegen_error)?;
    flags.set("unwind_info", "false").map_err(codegen_error)?;
    let triple = NATIVE_OBJECT_TARGET.parse::<target_lexicon::Triple>().map_err(codegen_error)?;
    let isa = cranelift_codegen::isa::lookup(triple)
        .map_err(codegen_error)?
        .finish(settings::Flags::new(flags))
        .map_err(codegen_error)?;
    let mut object_builder =
        ObjectBuilder::new(isa, b"zryna-native-c-exports-v0".to_vec(), default_libcall_names())
            .map_err(codegen_error)?;
    object_builder.per_function_section(false);
    let mut object = ObjectModule::new(object_builder);
    let mut builder_context = FunctionBuilderContext::new();
    let operations = program.operations().collect::<Vec<_>>();
    let header = scalar::header(program)?;
    for (ordinal, function) in program.functions().enumerate() {
        let Some(export) = function.export() else { continue };
        let operation = operations.get(export).ok_or_else(invariant_error)?;
        let signature = scalar::signature(operation.signature())?;
        let id = object
            .declare_function(&operation.declaration().symbol, Linkage::Export, &signature)
            .map_err(codegen_error)?;
        let mut context = Context::for_function(Function::with_name_signature(
            UserFuncName::user(0, u32::try_from(ordinal).map_err(codegen_error)?),
            signature,
        ));
        scalar::build(
            function,
            operation.signature(),
            &mut context,
            &mut builder_context,
            object.target_config(),
        )?;
        object.define_function(id, &mut context).map_err(codegen_error)?;
    }
    let bytes = object.finish().emit().map_err(codegen_error)?;
    audit::check(&bytes, program)?;
    Ok(ValidatedScalarExports { bytes, header, program: program.clone() })
}

fn invariant_error() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N3002",
        None,
        "native C export emission rejected a sealed invariant",
        "report the exact compiler revision and original source",
    )
}
fn codegen_error(error: impl std::fmt::Display) -> Diagnostic {
    let _ = error;
    Diagnostic::error(
        "ZRYNA-N3002",
        None,
        "native C export code generation failed",
        "report the exact compiler revision and original source",
    )
}
fn audit_error() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N3003",
        None,
        "native C exports failed the closed ELF object audit",
        "report the exact compiler revision and original source",
    )
}

#[cfg(test)]
mod tests;
