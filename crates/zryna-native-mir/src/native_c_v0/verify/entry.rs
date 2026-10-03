//! Independent private-entry replay from original source; no producer routine is an oracle.

use super::super::{
    MirError,
    abi::{Location, Register},
    entry::{ChannelLane, ChannelRole},
    raw, require,
};
use std::collections::BTreeSet;
use zryna_native_c_ir::VerifiedNativeCProgram;

pub(super) fn check(
    program: &raw::Program,
    source: &VerifiedNativeCProgram,
) -> Result<(), MirError> {
    let dispatch = &program.dispatcher;
    require(dispatch.symbol == "zryna_c_v0_i_dispatch", "ZRYNA-C4102", "mir-dispatch-symbol")?;
    require(
        dispatch.contract == "zryna-native-c-private-entry-v0"
            && dispatch.result_bits == 32
            && dispatch.stack_alignment == 16
            && dispatch.process_failures_out_of_band,
        "ZRYNA-C4104",
        "mir-private-outcome-contract",
    )?;
    lanes(&dispatch.parameters[..3])?;
    require(
        dispatch.parameters[3].role == ChannelRole::FunctionOrdinal
            && dispatch.parameters[3].bits == 32
            && dispatch.parameters[3].location == Location::Register(Register::Rcx),
        "ZRYNA-C4104",
        "mir-dispatch-ordinal",
    )?;
    let mut symbols = BTreeSet::new();
    symbols.insert(dispatch.symbol.to_ascii_lowercase());
    for operation in source.operations() {
        require(
            symbols.insert(operation.declaration().symbol.to_ascii_lowercase()),
            "ZRYNA-C4102",
            "mir-machine-symbol-collision",
        )?;
    }
    for (claim, original) in program.functions.iter().zip(source.functions()) {
        let (file, ordinal) = original.identity();
        let expected =
            ["zryna_c_v0_i_", &file.index().to_string(), "_", &ordinal.to_string()].concat();
        require(claim.entry.symbol == expected, "ZRYNA-C4102", "mir-original-private-symbol")?;
        require(
            symbols.insert(claim.entry.symbol.to_ascii_lowercase()),
            "ZRYNA-C4102",
            "mir-machine-symbol-collision",
        )?;
        require(
            claim.entry.contract == "zryna-native-c-private-entry-v0"
                && claim.entry.result_bits == 32
                && claim.entry.stack_alignment == 16
                && claim.entry.process_failures_out_of_band,
            "ZRYNA-C4104",
            "mir-private-outcome-contract",
        )?;
        lanes(&claim.entry.parameters)?;
    }
    Ok(())
}
fn lanes(lanes: &[ChannelLane]) -> Result<(), MirError> {
    for (index, lane) in lanes.iter().enumerate() {
        let (role, register) = match index {
            0 => (ChannelRole::Context, Register::Rdi),
            1 => (ChannelRole::Inputs, Register::Rsi),
            2 => (ChannelRole::Outcome, Register::Rdx),
            _ => return Err(MirError::new("ZRYNA-C4104", "mir-private-channel-count")),
        };
        require(
            lane.role == role && lane.bits == 64 && lane.location == Location::Register(register),
            "ZRYNA-C4104",
            "mir-private-channel-lane",
        )?;
    }
    Ok(())
}
