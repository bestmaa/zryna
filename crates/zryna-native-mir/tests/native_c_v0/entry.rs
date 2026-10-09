//! Fixed private-entry witnesses and independent mutations, without runtime execution claims.

use super::capture;
use zryna_native_mir::native_c_v0::{
    abi::{Location, Register},
    entry::{ChannelRole, OutcomeTag},
    lower, lower_unverified,
    raw::Program,
    verify,
};

fn reject(mutate: impl FnOnce(&mut Program), code: &str) {
    let (_, source) = capture::reference();
    let mut claim = lower_unverified(&source).expect("original machine candidate");
    mutate(&mut claim);
    assert_eq!(verify(claim, &source).expect_err("no private entry seal").code(), code);
}

#[test]
fn private_channels_retain_genuine_layouts_without_changing_public_c_signatures() {
    let (_, source) = capture::reference();
    let machine = lower(&source).expect("independent private entry admission");
    let dispatch = machine.dispatcher();
    assert_eq!(dispatch.symbol, "zryna_c_v0_i_dispatch");
    assert_eq!(dispatch.parameters[3].role, ChannelRole::FunctionOrdinal);
    assert_eq!(dispatch.parameters[3].bits, 32);
    assert_eq!(dispatch.parameters[3].location, Location::Register(Register::Rcx));
    assert!(dispatch.process_failures_out_of_band);
    for (function, original) in machine.functions().zip(source.functions()) {
        assert_eq!(function.parameters(), original.parameters());
        assert_eq!(function.result(), original.result());
        assert_eq!(function.entry().contract, "zryna-native-c-private-entry-v0");
        assert_eq!(function.entry().result_bits, 32);
        assert_eq!(function.entry().stack_alignment, 16);
        assert!(function.entry().process_failures_out_of_band);
        let (file, ordinal) = original.identity();
        assert_eq!(function.entry().symbol, format!("zryna_c_v0_i_{}_{ordinal}", file.index()));
        for ((lane, role), register) in function
            .entry()
            .parameters
            .iter()
            .zip([ChannelRole::Context, ChannelRole::Inputs, ChannelRole::Outcome])
            .zip([Register::Rdi, Register::Rsi, Register::Rdx])
        {
            assert_eq!(lane.role, role);
            assert_eq!(lane.bits, 64);
            assert_eq!(lane.location, Location::Register(register));
        }
    }
    let public = machine
        .functions()
        .find_map(zryna_native_mir::native_c_v0::VerifiedFunction::export)
        .expect("original scalar export");
    let operation = machine.operations().nth(public).expect("complete public operation");
    assert_eq!(operation.declaration().symbol, "zryna_c_v0_e_add");
    assert_eq!(operation.signature().parameters.len(), 2);
    assert!(operation.signature().parameters.iter().all(|lane| lane.bits == 32));
    assert_eq!(OutcomeTag::Returned.code(), 0);
    assert_eq!(OutcomeTag::ForeignError.code(), 1);
    assert_eq!(OutcomeTag::ControlledTrap.code(), 2);
    assert_eq!(OutcomeTag::HostAbiFailure.code(), 3);
}

#[test]
fn context_input_outcome_roles_and_physical_lanes_cannot_be_substituted() {
    reject(|p| p.functions[0].entry.parameters[0].role = ChannelRole::Inputs, "ZRYNA-C4104");
    reject(|p| p.functions[0].entry.parameters[1].bits = 32, "ZRYNA-C4104");
    reject(|p| p.functions[0].entry.parameters[2].location = Location::Stack(0), "ZRYNA-C4104");
    reject(
        |p| p.functions[0].entry.parameters[0].location = Location::Register(Register::R8),
        "ZRYNA-C4104",
    );
    reject(|p| p.functions[0].entry.result_bits = 64, "ZRYNA-C4104");
    reject(|p| p.functions[0].entry.stack_alignment = 8, "ZRYNA-C4104");
}

#[test]
fn dispatcher_ordinal_width_register_role_and_process_tag_cannot_change() {
    reject(|p| p.dispatcher.parameters[3].bits = 64, "ZRYNA-C4104");
    reject(
        |p| p.dispatcher.parameters[3].location = Location::Register(Register::Rsi),
        "ZRYNA-C4104",
    );
    reject(|p| p.dispatcher.parameters[3].role = ChannelRole::Context, "ZRYNA-C4104");
    reject(|p| p.dispatcher.parameters[0].role = ChannelRole::Outcome, "ZRYNA-C4104");
    reject(|p| p.dispatcher.process_failures_out_of_band = false, "ZRYNA-C4104");
    reject(|p| p.functions[0].entry.process_failures_out_of_band = false, "ZRYNA-C4104");
}

#[test]
fn entry_version_and_original_symbol_identity_cannot_be_renamed_or_reused() {
    reject(|p| p.functions[0].entry.contract.push('x'), "ZRYNA-C4104");
    reject(|p| p.dispatcher.contract.push('x'), "ZRYNA-C4104");
    reject(|p| p.functions[0].entry.symbol = p.functions[1].entry.symbol.clone(), "ZRYNA-C4102");
    reject(|p| p.functions[0].entry.symbol.make_ascii_uppercase(), "ZRYNA-C4102");
    reject(|p| p.dispatcher.symbol = "zryna_c_v0_e_add".into(), "ZRYNA-C4102");
}

#[test]
fn private_entry_payload_bounds_reject_before_machine_expansion() {
    reject(|p| p.functions[0].entry.symbol = "x".repeat(129), "ZRYNA-C4107");
    reject(|p| p.functions[0].entry.contract = "x".repeat(129), "ZRYNA-C4107");
    reject(|p| p.dispatcher.symbol = "x".repeat(129), "ZRYNA-C4107");
    reject(|p| p.dispatcher.contract = "x".repeat(129), "ZRYNA-C4107");
}

#[test]
fn public_boolean_and_c_int_export_spellings_survive_private_entry_planning() {
    for (source, spelling, count) in [
        (capture::constant_export(), zryna_native_mir::native_c_v0::contract::AbiType::CI32, 0),
        (capture::boolean_export(), zryna_native_mir::native_c_v0::contract::AbiType::Bool32, 1),
        (capture::c_int_export(), zryna_native_mir::native_c_v0::contract::AbiType::CInt, 2),
    ] {
        let machine = lower(&source).expect("genuine different scalar spelling");
        let function =
            machine.functions().find(|function| function.export().is_some()).expect("export");
        let operation =
            machine.operations().nth(function.export().expect("ordinal")).expect("operation");
        assert_eq!(operation.signature().parameters.len(), count);
        assert!(
            operation
                .signature()
                .parameters
                .iter()
                .all(|lane| lane.abi == spelling && lane.bits == 32)
        );
        assert_eq!(operation.signature().result.expect("scalar result").abi, spelling);
    }
}
