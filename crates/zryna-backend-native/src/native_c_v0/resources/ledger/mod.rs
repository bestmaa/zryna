//! Compiler-private native helpers. Foreign calls never receive this context pointer.

mod context;
mod owners;

use super::super::{codegen_error, invariant_error};
use cranelift_codegen::{
    Context,
    ir::{AbiParam, Function, InstBuilder, Signature, UserFuncName, types},
    isa::CallConv,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use std::collections::BTreeMap;
use zryna_diagnostics::Diagnostic;

pub(super) const ENTER: &str = "zryna_c_v0_r_handle_enter";
pub(super) const RESERVE: &str = "zryna_c_v0_r_handle_reserve";
pub(super) const REGISTER: &str = "zryna_c_v0_r_handle_register";
pub(super) const LOOKUP: &str = "zryna_c_v0_r_handle_lookup";
pub(super) const CONFIRM: &str = "zryna_c_v0_r_handle_confirm";
pub(super) const FINISH: &str = "zryna_c_v0_r_handle_finish";
pub(super) const SYMBOLS: [&str; 6] = [ENTER, RESERVE, REGISTER, LOOKUP, CONFIRM, FINISH];

pub(super) fn signature(name: &str) -> Result<Signature, Diagnostic> {
    let mut signature = Signature::new(CallConv::SystemV);
    let (parameters, result): (&[cranelift_codegen::ir::Type], _) = match name {
        ENTER => (&[types::I64, types::I64, types::I64, types::I32], types::I32),
        RESERVE => (&[types::I64, types::I32], types::I32),
        REGISTER => (&[types::I64, types::I64, types::I32, types::I32, types::I32], types::I64),
        LOOKUP => (
            &[types::I64, types::I64, types::I64, types::I32, types::I32, types::I32, types::I32],
            types::I32,
        ),
        CONFIRM => (&[types::I64, types::I64], types::I32),
        FINISH => (
            &[types::I64, types::I64, types::I32, types::I32, types::I32, types::I32, types::I32],
            types::I32,
        ),
        _ => return Err(invariant_error()),
    };
    signature.params.extend(parameters.iter().copied().map(AbiParam::new));
    signature.returns.push(AbiParam::new(result));
    Ok(signature)
}
pub(super) fn define(
    object: &mut ObjectModule,
    byte_channel: bool,
) -> Result<BTreeMap<&'static str, FuncId>, Diagnostic> {
    let mut ids = BTreeMap::new();
    let mut frontend = FunctionBuilderContext::new();
    for (ordinal, name) in SYMBOLS.into_iter().enumerate() {
        let signature = signature(name)?;
        let id =
            object.declare_function(name, Linkage::Local, &signature).map_err(codegen_error)?;
        let mut context = Context::for_function(Function::with_name_signature(
            UserFuncName::user(8, u32::try_from(ordinal).map_err(|_| invariant_error())?),
            signature,
        ));
        {
            let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            let parameters = builder.block_params(entry).to_vec();
            match name {
                ENTER => context::enter(
                    &mut builder,
                    &parameters,
                    if byte_channel { super::storage::MAGIC } else { super::header::MAGIC },
                ),
                RESERVE => context::reserve(&mut builder, &parameters),
                REGISTER => owners::register(&mut builder, &parameters),
                LOOKUP => owners::lookup(&mut builder, &parameters),
                CONFIRM => owners::confirm(&mut builder, &parameters),
                FINISH => context::finish(&mut builder, &parameters),
                _ => return Err(invariant_error()),
            }
            builder.seal_all_blocks();
            builder.finalize(object.target_config());
        }
        object.define_function(id, &mut context).map_err(codegen_error)?;
        ids.insert(name, id);
    }
    Ok(ids)
}

pub(super) fn require(
    builder: &mut FunctionBuilder<'_>,
    condition: cranelift_codegen::ir::Value,
    bad: cranelift_codegen::ir::Block,
) {
    let next = builder.create_block();
    builder.ins().brif(condition, next, &[], bad, &[]);
    builder.switch_to_block(next);
}
pub(super) fn boolean_return(builder: &mut FunctionBuilder<'_>, value: bool) {
    let value = builder.ins().iconst(types::I32, i64::from(value));
    builder.ins().return_(&[value]);
}
