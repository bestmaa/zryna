//! Only fixed native declarations from the exact retained generic runtime issuer are imported.
use super::{codegen, invariant};
use cranelift_codegen::{
    ir::{AbiParam, Signature, types},
    isa::CallConv,
};
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::generic_owned_v2::VerifiedProgram;
pub(super) const NAMES: [&str; 3] = ["stringFromUtf8Copy", "stringClone", "stringRelease"];
pub(super) fn names<'a>(p: &'a VerifiedProgram<'_, '_>) -> Result<Vec<&'a str>, Diagnostic> {
    NAMES
        .iter()
        .map(|name| {
            p.program()
                .runtime()
                .declarations()
                .native_linux_x86_64
                .iter()
                .find(|d| d.operation == *name)
                .map(|d| d.symbol.as_str())
                .ok_or_else(invariant)
        })
        .collect()
}
pub(super) fn declare(
    object: &mut ObjectModule,
    p: &VerifiedProgram<'_, '_>,
) -> Result<Vec<FuncId>, Diagnostic> {
    names(p)?
        .into_iter()
        .zip([3, 2, 1])
        .map(|(name, arity)| {
            let mut signature = Signature::new(CallConv::SystemV);
            for _ in 0..arity {
                signature.params.push(AbiParam::new(types::I64));
            }
            signature.returns.push(AbiParam::new(types::I32));
            object.declare_function(name, Linkage::Import, &signature).map_err(codegen)
        })
        .collect()
}
