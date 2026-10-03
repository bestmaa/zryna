//! Untaken byte outputs receive the same metadata and conditional-release checks as explicit takes.

use super::{
    super::{super::invariant_error, cleanup, state::State},
    copy,
};
use cranelift_codegen::ir::{InstBuilder, Value, condcodes::IntCC, types};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::contract::{CleanupEntry, Encoding};

pub(in crate::native_c_v0::resources) fn drop(
    state: &mut State<'_, '_>,
    owner: &CleanupEntry,
    tag: Value,
) -> Result<Value, Diagnostic> {
    let mut selected = tag;
    let next = state.builder.create_block();
    if owner.validation_required() {
        let policy = copy::policy(state, owner.owner_id())?.ok_or_else(invariant_error)?;
        let (_, status) = *state.calls.get(&owner.creating_call()).ok_or_else(invariant_error)?;
        let successful = state.builder.ins().icmp_imm_s(IntCC::Equal, status, 0);
        let mut valid = copy::valid_metadata(state, owner.owner_id(), &policy)?;
        if policy.encoding == Encoding::Utf8 {
            let pointer = copy::pointer(state, owner.owner_id())?;
            let length = copy::length(state, owner.owner_id())?;
            let present = state.builder.ins().icmp_imm_s(IntCC::NotEqual, pointer, 0);
            let read = state.builder.ins().band(successful, valid);
            let read = state.builder.ins().band(read, present);
            let entered = state.builder.create_block();
            let checked = state.builder.create_block();
            state.builder.append_block_param(checked, types::I32);
            let one = state.builder.ins().iconst(types::I32, 1);
            state.builder.ins().brif(read, entered, &[], checked, &[one.into()]);
            state.builder.switch_to_block(entered);
            let utf8 = state.helper(super::UTF8, &[pointer, length])?;
            state.builder.ins().jump(checked, &[utf8.into()]);
            state.builder.switch_to_block(checked);
            let utf8 = state.builder.block_params(checked)[0];
            let utf8 = state.builder.ins().icmp_imm_s(IntCC::Equal, utf8, 1);
            valid = state.builder.ins().band(valid, utf8);
        }
        let invalid = state.builder.ins().icmp_imm_s(IntCC::Equal, valid, 0);
        let malformed = state.builder.ins().band(successful, invalid);
        let host = state.builder.ins().iconst(types::I32, 3);
        selected = state.builder.ins().select(malformed, host, selected);
        if !owner.releasable_on_malformed() {
            let allowed = state.builder.create_block();
            state.builder.ins().brif(malformed, next, &[], allowed, &[]);
            state.builder.switch_to_block(allowed);
        }
    }
    cleanup::release(state, owner.owner_id(), true)?;
    state.builder.ins().jump(next, &[]);
    state.builder.switch_to_block(next);
    Ok(selected)
}
