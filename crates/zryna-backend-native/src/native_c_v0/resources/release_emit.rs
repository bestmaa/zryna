//! One nominal checked release body per required operation, shared by exact cleanup paths.

use super::{
    super::{codegen_error, invariant_error},
    admit, ledger,
};
use cranelift_codegen::{
    Context,
    ir::{AbiParam, Function, InstBuilder, Signature, UserFuncName, types},
    isa::CallConv,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use std::collections::{BTreeMap, BTreeSet};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::{VerifiedMirProgram, contract::FlowStep};

pub(super) fn symbol(operation: usize) -> String {
    format!("zryna_c_v0_r_release_{operation}")
}

pub(super) fn operations(program: &VerifiedMirProgram, selected: &[usize]) -> BTreeSet<usize> {
    let mut releases = BTreeSet::new();
    for (index, function) in program.functions().enumerate() {
        if !selected.contains(&index) {
            continue;
        }
        for effect in function.effects() {
            if let FlowStep::Call { created_owners, .. } = effect.operation() {
                for owner in created_owners {
                    if let Some(operation) = admit::release_for(function, program, *owner) {
                        releases.insert(operation);
                    }
                }
            }
        }
    }
    releases
}

pub(super) fn define(
    program: &VerifiedMirProgram,
    selected: &[usize],
    imports: &BTreeMap<usize, FuncId>,
    helpers: &BTreeMap<&'static str, FuncId>,
    object: &mut ObjectModule,
) -> Result<BTreeMap<usize, FuncId>, Diagnostic> {
    let mut definitions = BTreeMap::new();
    let mut frontend = FunctionBuilderContext::new();
    for operation in operations(program, selected) {
        let mut signature = Signature::new(CallConv::SystemV);
        signature.params.extend(
            [types::I64, types::I64, types::I64, types::I32, types::I32].map(AbiParam::new),
        );
        signature.returns.push(AbiParam::new(types::I32));
        let id = object
            .declare_function(&symbol(operation), Linkage::Local, &signature)
            .map_err(codegen_error)?;
        let mut context = Context::for_function(Function::with_name_signature(
            UserFuncName::user(12, u32::try_from(operation).map_err(|_| invariant_error())?),
            signature,
        ));
        let foreign = object.declare_func_in_func(
            *imports.get(&operation).ok_or_else(invariant_error)?,
            &mut context.func,
        );
        let lookup = object.declare_func_in_func(
            *helpers.get(ledger::LOOKUP).ok_or_else(invariant_error)?,
            &mut context.func,
        );
        let confirm = object.declare_func_in_func(
            *helpers.get(ledger::CONFIRM).ok_or_else(invariant_error)?,
            &mut context.func,
        );
        let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let arguments = builder.block_params(entry).to_vec();
        let bad = builder.create_block();
        let release = builder.ins().iconst(
            types::I32,
            i64::from(u32::try_from(operation).map_err(|_| invariant_error())?),
        );
        let mut checked = arguments.clone();
        checked.push(release);
        let begin = builder.ins().iconst(types::I32, 1);
        checked.push(begin);
        let call = builder.ins().call(lookup, &checked);
        let valid = builder.inst_results(call)[0];
        ledger::require(&mut builder, valid, bad);
        builder.ins().call(foreign, &[arguments[2]]);
        // Process faults never reach confirmation and cannot turn into private success tags.
        let returning = builder.ins().iconst(types::I32, 2);
        checked[6] = returning;
        let call = builder.ins().call(lookup, &checked);
        let valid = builder.inst_results(call)[0];
        ledger::require(&mut builder, valid, bad);
        let call = builder.ins().call(confirm, &arguments[..2]);
        let valid = builder.inst_results(call)[0];
        ledger::require(&mut builder, valid, bad);
        ledger::boolean_return(&mut builder, true);
        builder.switch_to_block(bad);
        ledger::boolean_return(&mut builder, false);
        builder.seal_all_blocks();
        builder.finalize(object.target_config());
        object.define_function(id, &mut context).map_err(codegen_error)?;
        definitions.insert(operation, id);
    }
    Ok(definitions)
}
