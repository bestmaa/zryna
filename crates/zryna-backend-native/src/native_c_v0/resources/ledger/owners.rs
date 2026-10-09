//! Live-record freshness, nominal release identity and one attempted/confirmed release transition.

use super::super::header::{CONTEXT_BYTES, RECORD_BYTES, RECORDS};
use super::{boolean_return, require};
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Value, condcodes::IntCC, types};
use cranelift_frontend::FunctionBuilder;

pub(super) fn register(builder: &mut FunctionBuilder<'_>, arguments: &[Value]) {
    let [context, pointer, function, owner, release] = arguments else {
        return;
    };
    let bad = builder.create_block();
    let nonnull = builder.ins().icmp_imm_s(IntCC::NotEqual, *pointer, 0);
    require(builder, nonnull, bad);
    let reserved = builder.ins().load(types::I32, MemFlagsData::new(), *context, 16);
    let credit = builder.ins().icmp_imm_s(IntCC::UnsignedGreaterThan, reserved, 0);
    require(builder, credit, bad);
    let live = builder.ins().load(types::I32, MemFlagsData::new(), *context, 12);
    let capacity = builder.ins().icmp_imm_u(IntCC::UnsignedLessThan, live, 64);
    require(builder, capacity, bad);
    let head = builder.create_block();
    let done = builder.create_block();
    for block in [head, done] {
        builder.append_block_param(block, types::I32);
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, types::I32);
    }
    let zero = builder.ins().iconst(types::I32, 0);
    let null = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(head, &[zero.into(), null.into(), zero.into()]);
    builder.switch_to_block(head);
    let parameters = builder.block_params(head).to_vec();
    let [index, first, count] = parameters.as_slice() else {
        return;
    };
    let body = builder.create_block();
    let bounded = builder.ins().icmp_imm_u(IntCC::UnsignedLessThan, *index, 64);
    builder.ins().brif(bounded, body, &[], done, &[*index, *first, *count].map(Into::into));
    builder.switch_to_block(body);
    let record = super::context::record_address(builder, *context, *index);
    let existing = builder.ins().load(types::I64, MemFlagsData::new(), record, 0);
    let state = builder.ins().load(types::I32, MemFlagsData::new(), record, 20);
    let valid_state = builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, state, 2);
    require(builder, valid_state, bad);
    let active = builder.ins().icmp_imm_s(IntCC::NotEqual, state, 0);
    let existing_nonnull = builder.ins().icmp_imm_s(IntCC::NotEqual, existing, 0);
    let consistent = builder.ins().icmp(IntCC::Equal, active, existing_nonnull);
    require(builder, consistent, bad);
    let equal_pointer = builder.ins().icmp(IntCC::Equal, existing, *pointer);
    let duplicate = builder.ins().band(active, equal_pointer);
    let fresh = builder.ins().icmp_imm_s(IntCC::Equal, duplicate, 0);
    require(builder, fresh, bad);
    let empty = builder.ins().icmp_imm_s(IntCC::Equal, state, 0);
    let no_first = builder.ins().icmp_imm_s(IntCC::Equal, *first, 0);
    let choose = builder.ins().band(empty, no_first);
    let first = builder.ins().select(choose, record, *first);
    let one = builder.ins().iconst(types::I32, 1);
    let increment = builder.ins().select(active, one, zero);
    let count = builder.ins().iadd(*count, increment);
    let index = builder.ins().iadd_imm_s(*index, 1);
    builder.ins().jump(head, &[index.into(), first.into(), count.into()]);
    builder.switch_to_block(done);
    let parameters = builder.block_params(done).to_vec();
    let [_, first, count] = parameters.as_slice() else {
        return;
    };
    let coherent_count = builder.ins().icmp(IntCC::Equal, *count, live);
    require(builder, coherent_count, bad);
    let available = builder.ins().icmp_imm_s(IntCC::NotEqual, *first, 0);
    require(builder, available, bad);
    builder.ins().store(MemFlagsData::new(), *pointer, *first, 0);
    for (offset, field) in [(8, *function), (12, *owner), (16, *release)] {
        builder.ins().store(MemFlagsData::new(), field, *first, offset);
    }
    let one = builder.ins().iconst(types::I32, 1);
    builder.ins().store(MemFlagsData::new(), one, *first, 20);
    let next_live = builder.ins().iadd_imm_s(live, 1);
    let credit = builder.ins().iadd_imm_s(reserved, -1);
    builder.ins().store(MemFlagsData::new(), next_live, *context, 12);
    builder.ins().store(MemFlagsData::new(), credit, *context, 16);
    builder.ins().return_(&[*first]);
    builder.switch_to_block(bad);
    let null = builder.ins().iconst(types::I64, 0);
    builder.ins().return_(&[null]);
}

fn valid_record(
    builder: &mut FunctionBuilder<'_>,
    context: Value,
    record: Value,
    bad: cranelift_codegen::ir::Block,
) {
    let start = builder.ins().iadd_imm_u(context, i64::from(RECORDS));
    let end = builder.ins().iadd_imm_u(context, i64::from(CONTEXT_BYTES));
    let lower = builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, record, start);
    require(builder, lower, bad);
    let upper = builder.ins().icmp(IntCC::UnsignedLessThan, record, end);
    require(builder, upper, bad);
    let offset = builder.ins().isub(record, start);
    let stride = builder.ins().iconst(types::I64, i64::from(RECORD_BYTES));
    let remainder = builder.ins().urem(offset, stride);
    let aligned = builder.ins().icmp_imm_s(IntCC::Equal, remainder, 0);
    require(builder, aligned, bad);
}

pub(super) fn lookup(builder: &mut FunctionBuilder<'_>, arguments: &[Value]) {
    let [context, record, pointer, function, owner, release, phase] = arguments else {
        return;
    };
    let bad = builder.create_block();
    valid_record(builder, *context, *record, bad);
    let bounded_phase = builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, *phase, 2);
    require(builder, bounded_phase, bad);
    let one = builder.ins().iconst(types::I32, 1);
    let two = builder.ins().iconst(types::I32, 2);
    let after_call = builder.ins().icmp_imm_s(IntCC::Equal, *phase, 2);
    let expected = builder.ins().select(after_call, two, one);
    let state = builder.ins().load(types::I32, MemFlagsData::new(), *record, 20);
    let matching_state = builder.ins().icmp(IntCC::Equal, state, expected);
    require(builder, matching_state, bad);
    for (offset, value, ty) in [
        (0, *pointer, types::I64),
        (8, *function, types::I32),
        (12, *owner, types::I32),
        (16, *release, types::I32),
    ] {
        let retained = builder.ins().load(ty, MemFlagsData::new(), *record, offset);
        let matches = builder.ins().icmp(IntCC::Equal, retained, value);
        require(builder, matches, bad);
    }
    let nonnull = builder.ins().icmp_imm_s(IntCC::NotEqual, *pointer, 0);
    require(builder, nonnull, bad);
    let begin = builder.ins().icmp_imm_s(IntCC::Equal, *phase, 1);
    let next_state = builder.ins().select(begin, two, state);
    builder.ins().store(MemFlagsData::new(), next_state, *record, 20);
    boolean_return(builder, true);
    builder.switch_to_block(bad);
    boolean_return(builder, false);
}

pub(super) fn confirm(builder: &mut FunctionBuilder<'_>, arguments: &[Value]) {
    let [context, record] = arguments else {
        return;
    };
    let bad = builder.create_block();
    valid_record(builder, *context, *record, bad);
    let state = builder.ins().load(types::I32, MemFlagsData::new(), *record, 20);
    let releasing = builder.ins().icmp_imm_s(IntCC::Equal, state, 2);
    require(builder, releasing, bad);
    let live = builder.ins().load(types::I32, MemFlagsData::new(), *context, 12);
    let present = builder.ins().icmp_imm_s(IntCC::UnsignedGreaterThan, live, 0);
    require(builder, present, bad);
    let bounded = builder.ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, live, 64);
    require(builder, bounded, bad);
    let zero = builder.ins().iconst(types::I64, 0);
    for offset in [0, 8, 16] {
        builder.ins().store(MemFlagsData::new(), zero, *record, offset);
    }
    let remaining = builder.ins().iadd_imm_s(live, -1);
    builder.ins().store(MemFlagsData::new(), remaining, *context, 12);
    boolean_return(builder, true);
    builder.switch_to_block(bad);
    boolean_return(builder, false);
}
