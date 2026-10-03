//! Native C machine boundary oracles, including independent hostile claims.

#[path = "native_c_v0/capture.rs"]
mod capture;
#[path = "native_c_v0/entry.rs"]
mod entry;
#[path = "native_c_v0/hostile.rs"]
mod hostile;

use zryna_native_c_ir::contract::*;
use zryna_native_mir::native_c_v0::{abi::*, lower, lower_unverified, raw::*, verify};

#[test]
fn original_complete_inventory_and_actual_issuers_survive_machine_admission() {
    let (sources, source) = capture::reference();
    let mir = lower(&source).expect("independent MIR");
    assert_eq!(mir.functions().len(), 6);
    assert_eq!(mir.operations().len(), 8);
    assert!(mir.source().belongs_to(&sources));
    assert_eq!(mir.source().source_map_identity(), sources.identity());
    assert_eq!(mir.source().native_layouts().fingerprint(), source.native_layouts().fingerprint());
    assert_eq!(mir.source().runtime_abi().identity(), source.runtime_abi().identity());
    assert_eq!(
        mir.source()
            .private_authority()
            .body_authority()
            .declaration_authority()
            .header_bytes("fixture-c-v0@0"),
        source
            .private_authority()
            .body_authority()
            .declaration_authority()
            .header_bytes("fixture-c-v0@0")
    );
    assert_eq!(mir.functions().filter(|function| function.export().is_some()).count(), 1);
}

#[test]
fn exact_integer_argument_and_low_width_result_lanes_match_the_c_fixture() {
    let (_, source) = capture::reference();
    let mir = lower(&source).expect("MIR");
    let add =
        mir.operations().find(|operation| operation.declaration().symbol == "add").expect("add");
    assert_eq!(
        add.signature().parameters,
        [
            Lane {
                abi: AbiType::CI32,
                bits: 32,
                location: Location::Register(Register::Rdi),
                canonical_bool: false
            },
            Lane {
                abi: AbiType::CI32,
                bits: 32,
                location: Location::Register(Register::Rsi),
                canonical_bool: false
            },
        ]
    );
    assert_eq!(
        add.signature().result,
        Some(ResultLane { abi: AbiType::CI32, bits: 32, canonical_bool: false })
    );
    let copy = mir
        .operations()
        .find(|operation| operation.declaration().symbol == "fixture_copy_bytes")
        .expect("copy");
    assert_eq!(
        copy.signature().parameters.iter().map(|lane| lane.bits).collect::<Vec<_>>(),
        [64, 64, 64, 64]
    );
    assert_eq!(copy.signature().stack_alignment, 16);
    assert_eq!(copy.signature().outgoing_bytes, 0);
}

#[test]
fn output_slots_are_distinct_aligned_zeroed_and_logically_uninitialized() {
    let (_, source) = capture::reference();
    let mir = lower(&source).expect("MIR");
    let copied = mir.functions().find(|function| function.name() == "copied").expect("copied");
    assert_eq!(copied.slots().len(), 2);
    assert_eq!(
        copied
            .slots()
            .iter()
            .map(|slot| (slot.offset, slot.bytes, slot.alignment))
            .collect::<Vec<_>>(),
        [(0, 8, 8), (8, 8, 8)]
    );
    assert_eq!(copied.output_frame_bytes(), 16);
    assert!(copied.slots().iter().all(|slot| slot.zeroed && !slot.initialized_at_entry));
    let scalar = mir.functions().find(|function| function.name() == "sum").expect("sum");
    assert_eq!(
        (scalar.slots()[0].offset, scalar.slots()[0].bytes, scalar.slots()[0].alignment),
        (0, 4, 4)
    );
    assert_eq!(scalar.output_frame_bytes(), 16);
}

#[test]
fn private_packing_copy_and_runtime_faults_keep_exact_machine_order() {
    let (_, source) = capture::reference();
    let mir = lower(&source).expect("MIR");
    let copied = mir.functions().find(|function| function.name() == "copied").expect("copied");
    let loan = copied
        .effects()
        .find(|effect| matches!(effect.preparation(), Some(PrivatePreparation::Loan(_))))
        .expect("loan");
    let Some(PrivatePreparation::Loan(plan)) = loan.preparation() else { unreachable!() };
    assert_eq!((plan.source_stride, plan.backing_stride, plan.backing_alignment), (4, 1, 1));
    assert!(matches!(loan.instructions()[0], Instruction::Storage(StorageStage::Length)));
    assert!(loan.instructions().contains(&Instruction::Storage(StorageStage::ByteRange)));
    assert!(loan.instructions().contains(&Instruction::Storage(StorageStage::AllocateNonempty)));
    assert!(!plan.faults.is_empty());
    let copy = copied
        .effects()
        .find(|effect| matches!(effect.preparation(), Some(PrivatePreparation::Copy(_))))
        .expect("copy");
    let Some(PrivatePreparation::Copy(plan)) = copy.preparation() else { unreachable!() };
    assert_eq!((plan.stride, plan.alignment), (4, 4));
    assert!(plan.zero_extend_bytes && plan.empty_without_allocation);
    assert_eq!(copy.instructions()[0], Instruction::Storage(StorageStage::ValidateForeign));
    assert!(copy.exits().iter().any(|exit| matches!(exit.kind, BoundaryExitKind::PrivateTrap(_))));
}

#[test]
fn reservations_precede_entry_and_status_registration_precedes_output_exposure() {
    let (_, source) = capture::reference();
    let mir = lower(&source).expect("MIR");
    let copied = mir.functions().find(|function| function.name() == "copied").expect("copied");
    let effects = copied.effects().collect::<Vec<_>>();
    let index = effects
        .iter()
        .position(|effect| matches!(effect.operation(), FlowStep::Call { .. }))
        .expect("call");
    assert!(matches!(
        effects[index - 1].instructions()[0],
        Instruction::Reserve { maximum: 1, limit: 64, .. }
    ));
    let actions = effects[index].instructions();
    let entry = actions
        .iter()
        .position(|action| matches!(action, Instruction::Invoke { .. }))
        .expect("entry");
    assert!(matches!(actions[entry + 1], Instruction::ClassifyStatus { .. }));
    assert!(matches!(actions[entry + 2], Instruction::StatusZeroRegister { .. }));
    assert!(matches!(
        actions[entry + 3],
        Instruction::SettleKnownStatusReservation { maximum: 1, .. }
    ));
    assert!(matches!(actions[entry + 4], Instruction::StatusZeroOutputs { .. }));
    assert!(
        effects.iter().any(|effect| matches!(effect.operation(), FlowStep::ConfirmRelease { .. }))
    );
}

#[test]
fn terminal_actions_end_loans_then_release_and_stop_before_result_transfer() {
    let (_, source) = capture::reference();
    let mir = lower(&source).expect("MIR");
    let mut releases = 0;
    let mut process = 0;
    let mut protected = 0;
    for effect in mir.functions().flat_map(zryna_native_mir::native_c_v0::VerifiedFunction::effects)
    {
        for (exit, actions) in effect.exits().iter().zip(effect.exit_instructions()) {
            assert_eq!(actions.last(), Some(&ExitInstruction::Finish));
            for (index, action) in actions.iter().enumerate() {
                if matches!(action, ExitInstruction::Release { .. }) {
                    releases += 1;
                    assert_eq!(actions[index + 1], ExitInstruction::StopOnReleaseFailure);
                }
            }
            if !exit.cleanup_required {
                process += 1;
                assert_eq!(actions, &[ExitInstruction::Finish]);
            }
            if exit.protected_result.is_some() {
                protected += 1;
            }
        }
    }
    assert!(releases > 0 && process > 0 && protected > 0);
}

#[test]
fn seven_eight_and_nine_argument_original_exports_use_stack_lanes_and_padding() {
    for (count, outgoing) in [(7, 16), (8, 16), (9, 32)] {
        let source = capture::stack_export(count);
        let mir = lower(&source).expect("original >6 argument MIR");
        let export = mir
            .operations()
            .find(|operation| operation.declaration().direction == Direction::Export)
            .expect("export");
        let signature = export.signature();
        assert_eq!(signature.parameters[5].location, Location::Register(Register::R9));
        assert_eq!(signature.parameters[6].location, Location::Stack(0));
        if count > 7 {
            assert_eq!(signature.parameters[7].location, Location::Stack(8));
        }
        if count > 8 {
            assert_eq!(signature.parameters[8].location, Location::Stack(16));
        }
        assert_eq!(signature.outgoing_bytes, outgoing);
        assert_eq!(signature.stack_alignment, 16);
        let mut bad = lower_unverified(&source).expect("raw");
        let export = bad
            .operations
            .iter_mut()
            .find(|operation| operation.declaration.direction == Direction::Export)
            .expect("export");
        export.signature.parameters[6].location = Location::Register(Register::R9);
        assert_eq!(
            verify(bad, &source).expect_err("seventh argument cannot alias sixth register").code(),
            "ZRYNA-C4104"
        );
    }
}

#[test]
fn boolean_shim_uses_low_32_bits_and_both_machine_boundary_checks() {
    let source = capture::boolean_import();
    let mir = lower(&source).expect("Bool32 MIR");
    let operation = mir
        .operations()
        .find(|operation| operation.declaration().symbol == "fixture_boolean_shim")
        .expect("Boolean");
    assert_eq!(
        (
            operation.signature().parameters[0].bits,
            operation.signature().parameters[0].canonical_bool
        ),
        (32, true)
    );
    assert_eq!(
        operation.signature().result,
        Some(ResultLane { abi: AbiType::Bool32, bits: 32, canonical_bool: true })
    );
    for defect in 0..3 {
        let mut bad = lower_unverified(&source).expect("raw");
        if defect == 0 {
            bad.operations
                .iter_mut()
                .find(|operation| operation.declaration.symbol == "fixture_boolean_shim")
                .expect("op")
                .signature
                .parameters[0]
                .bits = 64;
        } else {
            let function = bad
                .functions
                .iter_mut()
                .find(|function| function.name == "booleanBridge")
                .expect("bridge");
            let effect = function
                .effects
                .iter_mut()
                .find(|effect| matches!(effect.operation, FlowStep::Call { .. }))
                .expect("call");
            effect.instructions.retain(|action| {
                if defect == 1 {
                    !matches!(action, Instruction::Check(BoundaryCheck::BooleanCarrier { .. }))
                } else {
                    !matches!(action, Instruction::Check(BoundaryCheck::BooleanResult))
                }
            });
        }
        assert_eq!(
            verify(bad, &source).expect_err("low-width and input/result checks mandatory").code(),
            if defect == 0 { "ZRYNA-C4104" } else { "ZRYNA-C4106" }
        );
    }
}
