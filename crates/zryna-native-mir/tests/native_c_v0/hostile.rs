//! Hostile machine mutations use fixed independent rejection categories, never producer output.

use super::capture;
use zryna_native_c_ir::{contract::*, raw::ValueKind};
use zryna_native_mir::native_c_v0::{abi::*, lower_unverified, raw::*, verify};

fn reject(mutate: impl FnOnce(&mut Program), code: &str) {
    let (_, source) = capture::reference();
    let mut raw = lower_unverified(&source).expect("genuine untrusted baseline");
    mutate(&mut raw);
    let error = verify(raw, &source).expect_err("no seal may survive an independent mutation");
    assert_eq!(error.code(), code);
    assert_ne!(error.code(), "ZRYNA-C4108");
}
fn call(program: &mut Program) -> &mut Effect {
    program
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.effects)
        .find(|effect| matches!(effect.operation, FlowStep::Call { .. }))
        .expect("actual call")
}

#[test]
fn equal_source_bytes_from_another_map_cannot_substitute_for_original_issuer() {
    let (_, other) = capture::reference();
    reject(|program| program.source_map = other.source_map_identity(), "ZRYNA-C4102");
}
#[test]
fn unsupported_target_and_substituted_layout_or_runtime_descriptors_fail_closed() {
    reject(|program| program.target = "x86_64-pc-windows-msvc".into(), "ZRYNA-C4103");
    reject(|program| program.storage.native[0] ^= 1, "ZRYNA-C4102");
    reject(|program| program.storage.runtime.push('x'), "ZRYNA-C4102");
}
#[test]
fn changed_symbol_library_and_dropped_or_new_declarations_are_rejected() {
    reject(|program| program.operations[0].declaration.symbol = "unreviewed".into(), "ZRYNA-C4102");
    reject(|program| program.operations[0].declaration.library = "other@0".into(), "ZRYNA-C4102");
    reject(
        |program| {
            program.operations.pop();
        },
        "ZRYNA-C4102",
    );
    reject(|program| program.operations.push(program.operations[0].clone()), "ZRYNA-C4102");
}
#[test]
fn missing_reordered_and_renamed_functions_cannot_form_a_partial_program() {
    reject(
        |program| {
            program.functions.pop();
        },
        "ZRYNA-C4102",
    );
    reject(|program| program.functions.swap(0, 1), "ZRYNA-C4106");
    reject(|program| program.functions[0].name.push('x'), "ZRYNA-C4106");
}
#[test]
fn altered_values_arguments_and_status_provenance_are_source_rejected() {
    reject(|program| program.functions[0].values[0].kind = ValueKind::I32(999), "ZRYNA-C4106");
    reject(|program| program.functions[0].values[0].status_call = Some(999), "ZRYNA-C4106");
    reject(|program| program.functions[0].values[0].id = 999, "ZRYNA-C4106");
}
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
#[test]
fn machine_call_operand_order_and_entry_condition_cannot_be_substituted() {
    reject(
        |program| {
            let Instruction::Invoke { arguments, .. } = call(program)
                .instructions
                .iter_mut()
                .find(|action| matches!(action, Instruction::Invoke { .. }))
                .expect("invoke")
            else {
                unreachable!()
            };
            arguments.swap(0, 1);
        },
        "ZRYNA-C4106",
    );
    reject(
        |program| {
            let Instruction::Invoke { entry, .. } = call(program)
                .instructions
                .iter_mut()
                .find(|action| matches!(action, Instruction::Invoke { .. }))
                .expect("invoke")
            else {
                unreachable!()
            };
            *entry = CallEntry::NonEmptyOwner(999);
        },
        "ZRYNA-C4106",
    );
}
#[test]
fn erased_or_weakened_shared_reservation_before_c_entry_is_rejected() {
    for defect in 0..2 {
        reject(
            |program| {
                let effect = program.functions[0]
                    .effects
                    .iter_mut()
                    .find(|effect| matches!(effect.operation, FlowStep::Reserve { .. }))
                    .expect("reserve");
                if defect == 0 {
                    effect.instructions.remove(0);
                } else {
                    let Instruction::Reserve { limit, .. } = &mut effect.instructions[0] else {
                        unreachable!()
                    };
                    *limit = 65;
                }
            },
            "ZRYNA-C4106",
        );
    }
}
#[test]
fn registration_output_initialization_and_commits_cannot_move_before_status_check() {
    reject(
        |program| {
            call(program).instructions.retain(|action| {
                !matches!(action, Instruction::SettleKnownStatusReservation { .. })
            });
        },
        "ZRYNA-C4106",
    );
    reject(
        |program| {
            let effect = call(program);
            let classify = effect
                .instructions
                .iter()
                .position(|action| matches!(action, Instruction::ClassifyStatus { .. }))
                .expect("status");
            effect.instructions.swap(classify, classify + 1);
        },
        "ZRYNA-C4106",
    );
    reject(|program| call(program).instructions.rotate_right(1), "ZRYNA-C4106");
}
#[test]
fn erased_or_reordered_private_storage_machine_stages_are_rejected() {
    reject(
        |program| {
            let effect = program.functions[0]
                .effects
                .iter_mut()
                .find(|effect| effect.preparation.is_some())
                .expect("loan");
            effect.instructions.remove(0);
        },
        "ZRYNA-C4106",
    );
    reject(
        |program| {
            let effect = program.functions[0]
                .effects
                .iter_mut()
                .find(|effect| effect.preparation.is_some())
                .expect("loan");
            effect.instructions.swap(0, 1);
        },
        "ZRYNA-C4106",
    );
}
#[test]
fn private_fault_identity_and_stride_cannot_be_replaced_by_foreign_outcomes() {
    reject(
        |program| {
            let effect = program.functions[0]
                .effects
                .iter_mut()
                .find(|effect| effect.preparation.is_some())
                .expect("loan");
            let Some(PrivatePreparation::Loan(loan)) = &mut effect.preparation else {
                unreachable!()
            };
            loan.source_stride = 1;
        },
        "ZRYNA-C4106",
    );
    reject(
        |program| {
            let exit = program
                .functions
                .iter_mut()
                .flat_map(|function| &mut function.effects)
                .flat_map(|effect| &mut effect.exits)
                .find(|exit| matches!(exit.kind, BoundaryExitKind::PrivateTrap(_)))
                .expect("private fault");
            exit.kind = BoundaryExitKind::ForeignFailure(FailureRoute::DeclaredForeignError);
        },
        "ZRYNA-C4106",
    );
}
#[test]
fn dropped_terminal_edges_reordered_releases_and_missing_failure_stops_are_rejected() {
    reject(
        |program| {
            call(program).exit_instructions.pop();
        },
        "ZRYNA-C4106",
    );
    reject(
        |program| {
            let actions = program
                .functions
                .iter_mut()
                .flat_map(|function| &mut function.effects)
                .flat_map(|effect| &mut effect.exit_instructions)
                .find(|actions| {
                    actions.iter().any(|action| matches!(action, ExitInstruction::Release { .. }))
                })
                .expect("release exit");
            let index = actions
                .iter()
                .position(|action| matches!(action, ExitInstruction::Release { .. }))
                .expect("release");
            actions.swap(index, index + 1);
        },
        "ZRYNA-C4106",
    );
    reject(
        |program| {
            let effect = program
                .functions
                .iter_mut()
                .flat_map(|function| &mut function.effects)
                .find(|effect| matches!(effect.operation, FlowStep::ConfirmRelease { .. }))
                .expect("confirm");
            effect.instructions.remove(0);
        },
        "ZRYNA-C4106",
    );
}
#[test]
fn hostile_inventory_limits_reject_before_source_or_machine_expansion() {
    reject(|program| program.operations.resize(257, program.operations[0].clone()), "ZRYNA-C4107");
    reject(
        |program| call(program).instructions.resize(33, Instruction::CommitOwners(Vec::new())),
        "ZRYNA-C4107",
    );
    reject(
        |program| {
            let effect = call(program);
            effect.exit_instructions[0].resize(262_145, ExitInstruction::Finish);
        },
        "ZRYNA-C4107",
    );
}
