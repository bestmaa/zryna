//! Entry, shared reservation and typed terminal framing; no foreign allocation is adopted.

use super::super::header::{CONTEXT_BYTES, RECORD_BYTES, RECORDS};
use super::{boolean_return, require};
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Value, condcodes::IntCC, types};
use cranelift_frontend::FunctionBuilder;

pub(super) fn enter(builder: &mut FunctionBuilder<'_>, arguments: &[Value], magic_value: i64) {
    let [context, inputs, outcome, arity] = arguments else {
        return;
    };
    let bad = builder.create_block();
    let bad_output = builder.create_block();
    let valid_output = builder.ins().icmp_imm_s(IntCC::NotEqual, *outcome, 0);
    require(builder, valid_output, bad_output);
    let alignment = builder.ins().band_imm_u(*outcome, 3);
    let aligned = builder.ins().icmp_imm_s(IntCC::Equal, alignment, 0);
    require(builder, aligned, bad_output);
    let zero = builder.ins().iconst(types::I32, 0);
    for offset in (0..32).step_by(4) {
        builder.ins().store(MemFlagsData::new(), zero, *outcome, offset);
    }
    let tag = builder.ins().iconst(types::I32, 3);
    builder.ins().store(MemFlagsData::new(), tag, *outcome, 0);
    let no_operation = builder.ins().iconst(types::I32, -1);
    builder.ins().store(MemFlagsData::new(), no_operation, *outcome, 4);
    for (pointer, alignment) in [(*context, 7), (*inputs, 3)] {
        let nonnull = builder.ins().icmp_imm_s(IntCC::NotEqual, pointer, 0);
        require(builder, nonnull, bad);
        let low = builder.ins().band_imm_u(pointer, alignment);
        let aligned = builder.ins().icmp_imm_s(IntCC::Equal, low, 0);
        require(builder, aligned, bad);
    }
    let magic = builder.ins().load(types::I64, MemFlagsData::new(), *context, 0);
    let expected_magic = builder.ins().iconst(types::I64, magic_value);
    let exact_magic = builder.ins().icmp(IntCC::Equal, magic, expected_magic);
    require(builder, exact_magic, bad);
    let live = builder.ins().load(types::I32, MemFlagsData::new(), *context, 12);
    let reserved = builder.ins().load(types::I32, MemFlagsData::new(), *context, 16);
    let unresolved = builder.ins().iadd(live, reserved);
    builder.ins().store(MemFlagsData::new(), unresolved, *outcome, 20);
    builder.ins().store(MemFlagsData::new(), reserved, *outcome, 24);
    for offset in [8, 12, 16, 20] {
        let field = builder.ins().load(types::I32, MemFlagsData::new(), *context, offset);
        let empty = builder.ins().icmp_imm_s(IntCC::Equal, field, 0);
        require(builder, empty, bad);
    }
    let bounded_arity = builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, *arity, 16);
    require(builder, bounded_arity, bad);
    let count = builder.ins().load(types::I32, MemFlagsData::new(), *inputs, 0);
    let exact_arity = builder.ins().icmp(IntCC::Equal, count, *arity);
    require(builder, exact_arity, bad);
    let padding = builder.ins().load(types::I32, MemFlagsData::new(), *inputs, 4);
    let zero_padding = builder.ins().icmp_imm_s(IntCC::Equal, padding, 0);
    require(builder, zero_padding, bad);
    // Scalar-only boundary entries cannot receive live foreign owners. A poisoned previous
    // invocation is rejected above; no new invocation guesses how to dispose of its obligations.
    for offset in (RECORDS..CONTEXT_BYTES).step_by(8) {
        let field = builder.ins().load(types::I64, MemFlagsData::new(), *context, offset);
        let empty = builder.ins().icmp_imm_s(IntCC::Equal, field, 0);
        require(builder, empty, bad);
    }
    let busy = builder.ins().iconst(types::I32, 1);
    builder.ins().store(MemFlagsData::new(), busy, *context, 8);
    boolean_return(builder, true);
    builder.switch_to_block(bad);
    boolean_return(builder, false);
    builder.switch_to_block(bad_output);
    boolean_return(builder, false);
}

pub(super) fn reserve(builder: &mut FunctionBuilder<'_>, arguments: &[Value]) {
    let [context, maximum] = arguments else {
        return;
    };
    let bad = builder.create_block();
    let live = builder.ins().load(types::I32, MemFlagsData::new(), *context, 12);
    let reserved = builder.ins().load(types::I32, MemFlagsData::new(), *context, 16);
    for value in [live, reserved, *maximum] {
        let bounded = builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, value, 64);
        require(builder, bounded, bad);
    }
    let total = builder.ins().iadd(live, reserved);
    let next = builder.ins().iadd(total, *maximum);
    let available = builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, next, 64);
    require(builder, available, bad);
    let credit = builder.ins().iadd(reserved, *maximum);
    builder.ins().store(MemFlagsData::new(), credit, *context, 16);
    boolean_return(builder, true);
    builder.switch_to_block(bad);
    boolean_return(builder, false);
}

pub(super) fn finish(builder: &mut FunctionBuilder<'_>, arguments: &[Value]) {
    let [context, outcome, tag, operation, status, trap, value] = arguments else {
        return;
    };
    let zero = builder.ins().iconst(types::I32, 0);
    let returned = builder.ins().icmp_imm_s(IntCC::Equal, *tag, 0);
    let exposed = builder.ins().select(returned, *value, zero);
    for (offset, field) in
        [(0, *tag), (4, *operation), (8, *status), (12, *trap), (16, exposed), (28, zero)]
    {
        builder.ins().store(MemFlagsData::new(), field, *outcome, offset);
    }
    let live = builder.ins().load(types::I32, MemFlagsData::new(), *context, 12);
    let reserved = builder.ins().load(types::I32, MemFlagsData::new(), *context, 16);
    let unresolved = builder.ins().iadd(live, reserved);
    builder.ins().store(MemFlagsData::new(), unresolved, *outcome, 20);
    builder.ins().store(MemFlagsData::new(), reserved, *outcome, 24);
    builder.ins().store(MemFlagsData::new(), zero, *context, 8);
    let host_failure = builder.ins().icmp_imm_s(IntCC::Equal, *tag, 3);
    let one = builder.ins().iconst(types::I32, 1);
    let prior = builder.ins().load(types::I32, MemFlagsData::new(), *context, 20);
    let poisoned = builder.ins().select(host_failure, one, prior);
    builder.ins().store(MemFlagsData::new(), poisoned, *context, 20);
    builder.ins().return_(&[*tag]);
}

pub(super) fn record_address(
    builder: &mut FunctionBuilder<'_>,
    context: Value,
    index: Value,
) -> Value {
    let index = builder.ins().uextend(types::I64, index);
    let offset = builder.ins().imul_imm_s(index, i64::from(RECORD_BYTES));
    let base = builder.ins().iadd_imm_u(context, i64::from(RECORDS));
    builder.ins().iadd(base, offset)
}
