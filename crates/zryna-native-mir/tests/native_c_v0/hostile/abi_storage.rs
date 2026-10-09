//! Independent machine ABI lanes and output-storage mutations.

use super::reject;
use zryna_native_c_ir::contract::{AbiType, FlowStep};
use zryna_native_mir::native_c_v0::abi::{Location, Register};

#[test]
fn changed_argument_register_width_and_c_spelling_are_abi_rejected() {
    reject(
        |program| {
            program.operations[0].signature.parameters[0].location =
                Location::Register(Register::R9);
        },
        "ZRYNA-C4104",
    );
    reject(|program| program.operations[0].signature.parameters[0].bits = 64, "ZRYNA-C4104");
    reject(
        |program| program.operations[0].signature.parameters[0].abi = AbiType::CInt,
        "ZRYNA-C4104",
    );
}
#[test]
fn incorrect_outgoing_alignment_and_result_lane_are_abi_rejected() {
    reject(|program| program.operations[0].signature.stack_alignment = 8, "ZRYNA-C4104");
    reject(|program| program.operations[0].signature.outgoing_bytes = 16, "ZRYNA-C4104");
    reject(|program| program.operations[0].signature.result = None, "ZRYNA-C4104");
}
#[test]
fn overlapping_or_misaligned_output_slots_and_wrong_frame_size_are_rejected() {
    reject(
        |program| {
            let copied = program
                .functions
                .iter_mut()
                .find(|function| function.name == "copied")
                .expect("copied");
            copied.slots[1].offset = 0;
        },
        "ZRYNA-C4105",
    );
    reject(|program| program.functions[0].slots[0].alignment = 1, "ZRYNA-C4105");
    reject(|program| program.functions[0].output_frame_bytes = 0, "ZRYNA-C4105");
}
#[test]
fn unzeroed_or_early_initialized_slots_and_missing_zero_actions_are_rejected() {
    reject(|program| program.functions[0].slots[0].zeroed = false, "ZRYNA-C4105");
    reject(|program| program.functions[0].slots[0].initialized_at_entry = true, "ZRYNA-C4105");
    reject(
        |program| {
            let effect = program.functions[0]
                .effects
                .iter_mut()
                .find(|effect| matches!(effect.operation, FlowStep::OutputSlot { .. }))
                .expect("slot");
            effect.instructions.remove(0);
        },
        "ZRYNA-C4106",
    );
}
