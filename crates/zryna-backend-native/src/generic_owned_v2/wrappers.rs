//! Exact unchanged scalar ABI carriers; raw Boolean guards precede body execution.

use cranelift_codegen::{
    Context,
    ir::{
        AbiParam, FuncRef, InstBuilder, Signature, StackSlotData, StackSlotKind, TrapCode,
        condcodes::IntCC, types,
    },
    isa::CallConv,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use zryna_abi::{ScalarType, VerifiedScalarExport};

pub(super) fn signature(export: VerifiedScalarExport<'_>) -> Signature {
    let mut s = Signature::new(CallConv::SystemV);
    for _ in export.parameters() {
        s.params.push(AbiParam::new(types::I32));
    }
    s.returns.push(AbiParam::new(types::I32));
    s
}

pub(super) fn build(
    export: VerifiedScalarExport<'_>,
    callee: FuncRef,
    context: &mut Context,
    bc: &mut FunctionBuilderContext,
    config: cranelift_codegen::isa::TargetFrontendConfig,
) {
    let mut b = FunctionBuilder::new(&mut context.func, bc);
    let block = b.create_block();
    b.append_block_params_for_function_params(block);
    b.switch_to_block(block);
    b.seal_block(block);
    let values = b.block_params(block).to_vec();
    for (value, ty) in values.iter().zip(export.parameters()) {
        if *ty == ScalarType::Bool {
            guard(&mut b, *value);
        }
    }
    let slot = b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let mut arguments = vec![b.ins().stack_addr(types::I64, slot, 0)];
    arguments.extend(values.into_iter().map(|v| b.ins().uextend(types::I64, v)));
    let call = b.ins().call(callee, &arguments);
    let status = b.inst_results(call)[0];
    b.ins().trapnz(status, TrapCode::unwrap_user(7));
    let wide = b.ins().stack_load(types::I64, types::I64, slot, 0);
    let result = b.ins().ireduce(types::I32, wide);
    if export.result() == ScalarType::Bool {
        guard(&mut b, result);
    }
    b.ins().return_(&[result]);
    b.finalize(config);
}

fn guard(b: &mut FunctionBuilder<'_>, value: cranelift_codegen::ir::Value) {
    let invalid = b.ins().icmp_imm_u(IntCC::UnsignedGreaterThan, value, 1);
    b.ins().trapnz(invalid, TrapCode::unwrap_user(7));
}
