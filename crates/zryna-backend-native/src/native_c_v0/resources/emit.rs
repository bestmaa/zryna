//! Pinned Cranelift machine emission; foreign bodies are never substituted into Zryna entries.

use super::{
    super::{codegen_error, invariant_error},
    admit, ledger, lower, release_emit,
    state::Environment,
    storage,
};
use crate::{LinuxX8664ObjectTarget, NATIVE_OBJECT_TARGET};
use cranelift_codegen::{
    Context,
    ir::{AbiParam, Function, InstBuilder, Signature, UserFuncName, condcodes::IntCC, types},
    isa::CallConv,
    settings::{self, Configurable},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use std::collections::BTreeMap;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::{VerifiedMirProgram, abi, contract::AbiType};

pub(super) fn private_signature() -> Signature {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.extend([AbiParam::new(types::I64); 3]);
    signature.returns.push(AbiParam::new(types::I32));
    signature
}
fn foreign_signature(source: &abi::Signature) -> Result<Signature, Diagnostic> {
    let mut signature = Signature::new(CallConv::SystemV);
    for lane in &source.parameters {
        signature.params.push(AbiParam::new(if lane.bits == 32 { types::I32 } else { types::I64 }));
    }
    if let Some(result) = source.result {
        if !matches!(result.abi, AbiType::CI32 | AbiType::CInt | AbiType::Bool32) {
            return Err(invariant_error());
        }
        signature.returns.push(AbiParam::new(types::I32));
    }
    Ok(signature)
}

pub(super) fn object(
    program: &VerifiedMirProgram,
    selected: &[usize],
    _target: LinuxX8664ObjectTarget,
) -> Result<Vec<u8>, Diagnostic> {
    let mut flags = settings::builder();
    for (key, value) in [("opt_level", "none"), ("is_pic", "false"), ("unwind_info", "false")] {
        flags.set(key, value).map_err(codegen_error)?;
    }
    let triple = NATIVE_OBJECT_TARGET.parse::<target_lexicon::Triple>().map_err(codegen_error)?;
    let isa = cranelift_codegen::isa::lookup(triple)
        .map_err(codegen_error)?
        .finish(settings::Flags::new(flags))
        .map_err(codegen_error)?;
    let mut builder =
        ObjectBuilder::new(isa, b"zryna-native-c-handles-v0".to_vec(), default_libcall_names())
            .map_err(codegen_error)?;
    builder.per_function_section(false);
    let mut object = ObjectModule::new(builder);
    let byte_channel = storage::enabled(program, selected);
    let helpers = ledger::define(&mut object, byte_channel)?;
    let needed = admit::imports(program, selected);
    let mut imports = BTreeMap::new();
    for operation in program.operations() {
        if needed.contains(&operation.index()) {
            let id = object
                .declare_function(
                    &operation.declaration().symbol,
                    Linkage::Import,
                    &foreign_signature(operation.signature())?,
                )
                .map_err(codegen_error)?;
            imports.insert(operation.index(), id);
        }
    }
    let mut private_imports = BTreeMap::new();
    let required = storage::runtime_imports(program, selected);
    for runtime in program.source().runtime_abi().native_linux_x86_64_functions() {
        if required.contains(runtime.symbol()) {
            let signature = storage::runtime_signature(runtime)?;
            let id = object
                .declare_function(runtime.symbol(), Linkage::Import, &signature)
                .map_err(codegen_error)?;
            private_imports.insert(runtime.symbol().to_owned(), id);
        }
    }
    let storage_helpers = storage::define(program, selected, &mut object)?;
    let releases = release_emit::define(program, selected, &imports, &helpers, &mut object)?;
    let mut functions = BTreeMap::new();
    let mut frontend = FunctionBuilderContext::new();
    for (ordinal, function) in program.functions().enumerate() {
        if !selected.contains(&ordinal) {
            continue;
        }
        let signature = private_signature();
        let id = object
            .declare_function(&function.entry().symbol, Linkage::Hidden, &signature)
            .map_err(codegen_error)?;
        let mut context = Context::for_function(Function::with_name_signature(
            UserFuncName::user(9, u32::try_from(ordinal).map_err(|_| invariant_error())?),
            signature,
        ));
        let imports = imports
            .iter()
            .map(|(ordinal, id)| (*ordinal, object.declare_func_in_func(*id, &mut context.func)))
            .collect();
        let runtime = helpers
            .iter()
            .map(|(name, id)| (name.to_string(), *id))
            .chain(private_imports.iter().map(|(name, id)| (name.clone(), *id)))
            .chain(storage_helpers.iter().map(|(name, id)| (name.clone(), *id)))
            .map(|(name, id)| (name, object.declare_func_in_func(id, &mut context.func)))
            .collect();
        let releases = releases
            .iter()
            .map(|(operation, id)| {
                (*operation, object.declare_func_in_func(*id, &mut context.func))
            })
            .collect();
        lower::build(
            Environment { program, function, ordinal, imports, runtime, byte_channel, releases },
            &mut context,
            &mut frontend,
            object.target_config(),
        )?;
        object.define_function(id, &mut context).map_err(codegen_error)?;
        functions.insert(ordinal, id);
    }
    dispatcher(program, &functions, &helpers, &mut object, &mut frontend)?;
    object.finish().emit().map_err(codegen_error)
}

fn dispatcher(
    program: &VerifiedMirProgram,
    functions: &BTreeMap<usize, FuncId>,
    helpers: &BTreeMap<&'static str, FuncId>,
    object: &mut ObjectModule,
    frontend: &mut FunctionBuilderContext,
) -> Result<(), Diagnostic> {
    let mut signature = private_signature();
    signature.params.push(AbiParam::new(types::I32));
    let id = object
        .declare_function(&program.dispatcher().symbol, Linkage::Hidden, &signature)
        .map_err(codegen_error)?;
    let mut context =
        Context::for_function(Function::with_name_signature(UserFuncName::user(10, 0), signature));
    let references = functions
        .iter()
        .map(|(ordinal, id)| (*ordinal, object.declare_func_in_func(*id, &mut context.func)))
        .collect::<Vec<_>>();
    let enter = object.declare_func_in_func(
        *helpers.get(ledger::ENTER).ok_or_else(invariant_error)?,
        &mut context.func,
    );
    let mut builder = FunctionBuilder::new(&mut context.func, frontend);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    let parameters = builder.block_params(entry).to_vec();
    let [context_pointer, inputs, outcome, ordinal] = parameters.as_slice() else {
        return Err(invariant_error());
    };
    for (expected, reference) in references {
        let matched = builder.ins().icmp_imm_u(
            IntCC::Equal,
            *ordinal,
            i64::from(u32::try_from(expected).map_err(|_| invariant_error())?),
        );
        let selected = builder.create_block();
        let next = builder.create_block();
        builder.ins().brif(matched, selected, &[], next, &[]);
        builder.switch_to_block(selected);
        let call = builder.ins().call(reference, &[*context_pointer, *inputs, *outcome]);
        let result = builder.inst_results(call)[0];
        builder.ins().return_(&[result]);
        builder.switch_to_block(next);
    }
    // An unsupported complete-program ordinal never enters any source body or foreign call.
    let rejected_arity = builder.ins().iconst(types::I32, -1);
    builder.ins().call(enter, &[*context_pointer, *inputs, *outcome, rejected_arity]);
    let tag = builder.ins().iconst(types::I32, 3);
    builder.ins().return_(&[tag]);
    builder.seal_all_blocks();
    builder.finalize(object.target_config());
    object.define_function(id, &mut context).map_err(codegen_error)
}
