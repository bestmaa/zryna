//! Complete generated UTF-8 validation with a bounds check before every continuation read.

use super::super::super::codegen_error;
use cranelift_codegen::{
    Context,
    ir::{
        AbiParam, Function, InstBuilder, MemFlagsData, Signature, UserFuncName, condcodes::IntCC,
        types,
    },
    isa::CallConv,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use zryna_diagnostics::Diagnostic;

pub(in crate::native_c_v0::resources) fn define(
    object: &mut ObjectModule,
) -> Result<FuncId, Diagnostic> {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.extend([AbiParam::new(types::I64); 2]);
    signature.returns.push(AbiParam::new(types::I32));
    let id =
        object.declare_function(super::UTF8, Linkage::Local, &signature).map_err(codegen_error)?;
    let mut context =
        Context::for_function(Function::with_name_signature(UserFuncName::user(13, 0), signature));
    let mut frontend = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    let arguments = builder.block_params(entry).to_vec();
    let pointer = arguments[0];
    let length = arguments[1];
    let outer = builder.create_block();
    builder.append_block_param(outer, types::I64);
    let read = builder.create_block();
    let good = builder.create_block();
    let bad = builder.create_block();
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(outer, &[zero.into()]);
    builder.switch_to_block(outer);
    let index = builder.block_params(outer)[0];
    let remains = builder.ins().icmp(IntCC::UnsignedLessThan, index, length);
    builder.ins().brif(remains, read, &[], good, &[]);
    builder.switch_to_block(read);
    let address = builder.ins().iadd(pointer, index);
    let byte = builder.ins().load(types::I8, MemFlagsData::new(), address, 0);
    let byte = builder.ins().uextend(types::I32, byte);
    let next = builder.ins().iadd_imm_u(index, 1);
    let ascii = builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, byte, 127);
    let multibyte = builder.create_block();
    builder.ins().brif(ascii, outer, &[next.into()], multibyte, &[]);
    builder.switch_to_block(multibyte);
    let two = range(&mut builder, byte, 0xc2, 0xdf);
    let three = range(&mut builder, byte, 0xe0, 0xef);
    let four = range(&mut builder, byte, 0xf0, 0xf4);
    let allowed = builder.ins().bor(two, three);
    let allowed = builder.ins().bor(allowed, four);
    super::super::ledger::require(&mut builder, allowed, bad);
    let one = builder.ins().iconst(types::I32, 1);
    let second = builder.ins().iconst(types::I32, 2);
    let third = builder.ins().iconst(types::I32, 3);
    let count = builder.ins().select(three, second, third);
    let count = builder.ins().select(two, one, count);
    let mask2 = builder.ins().band_imm_u(byte, 0x1f);
    let mask3 = builder.ins().band_imm_u(byte, 0x0f);
    let mask4 = builder.ins().band_imm_u(byte, 0x07);
    let scalar = builder.ins().select(three, mask3, mask4);
    let scalar = builder.ins().select(two, mask2, scalar);
    let min2 = builder.ins().iconst(types::I32, 0x80);
    let min3 = builder.ins().iconst(types::I32, 0x800);
    let min4 = builder.ins().iconst(types::I32, 0x0001_0000);
    let minimum = builder.ins().select(three, min3, min4);
    let minimum = builder.ins().select(two, min2, minimum);
    let continuation = builder.create_block();
    for ty in [types::I64, types::I32, types::I32] {
        builder.append_block_param(continuation, ty);
    }
    let consume = builder.create_block();
    let validate = builder.create_block();
    builder.ins().jump(continuation, &[next.into(), count.into(), scalar.into()]);
    builder.switch_to_block(continuation);
    let current = builder.block_params(continuation)[0];
    let count = builder.block_params(continuation)[1];
    let scalar = builder.block_params(continuation)[2];
    builder.ins().brif(count, consume, &[], validate, &[]);
    builder.switch_to_block(consume);
    let bounded = builder.ins().icmp(IntCC::UnsignedLessThan, current, length);
    super::super::ledger::require(&mut builder, bounded, bad);
    let address = builder.ins().iadd(pointer, current);
    let byte = builder.ins().load(types::I8, MemFlagsData::new(), address, 0);
    let byte = builder.ins().uextend(types::I32, byte);
    let allowed = range(&mut builder, byte, 0x80, 0xbf);
    super::super::ledger::require(&mut builder, allowed, bad);
    let shifted = builder.ins().ishl_imm_u(scalar, 6);
    let low = builder.ins().band_imm_u(byte, 0x3f);
    let next_scalar = builder.ins().bor(shifted, low);
    let next = builder.ins().iadd_imm_u(current, 1);
    let remaining = builder.ins().iadd_imm_s(count, -1);
    builder.ins().jump(continuation, &[next.into(), remaining.into(), next_scalar.into()]);
    builder.switch_to_block(validate);
    let enough = builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, scalar, minimum);
    super::super::ledger::require(&mut builder, enough, bad);
    let bounded = builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, scalar, 0x0010_ffff);
    super::super::ledger::require(&mut builder, bounded, bad);
    let surrogate = range(&mut builder, scalar, 0xd800, 0xdfff);
    builder.ins().brif(surrogate, bad, &[], outer, &[current.into()]);
    builder.switch_to_block(good);
    super::super::ledger::boolean_return(&mut builder, true);
    builder.switch_to_block(bad);
    super::super::ledger::boolean_return(&mut builder, false);
    builder.seal_all_blocks();
    builder.finalize(object.target_config());
    object.define_function(id, &mut context).map_err(codegen_error)?;
    Ok(id)
}

fn range(
    builder: &mut FunctionBuilder<'_>,
    value: cranelift_codegen::ir::Value,
    low: i64,
    high: i64,
) -> cranelift_codegen::ir::Value {
    let above = builder.ins().icmp_imm_u(IntCC::UnsignedGreaterThanOrEqual, value, low);
    let below = builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, value, high);
    builder.ins().band(above, below)
}
