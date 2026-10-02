//! Source/material captures enter the genuine syntax, declaration and body boundaries.

use super::{FlowStep, TrapRequirement, ValueType, verify_bodies};

mod abi;
mod capture;
mod cleanup;
mod limits;
mod nominal;
mod private_boundary;
mod rejections;
mod slots;
mod status;

#[test]
fn native_c_body_v0_authenticates_every_reference_body_and_preserves_original_identity() {
    let capture = capture::reference();
    let bodies = verify_bodies(&capture.sources, &capture.declarations)
        .expect("complete typed foreign bodies");
    assert_eq!(bodies.functions().len(), 6);
    assert!(bodies.belongs_to(&capture.sources));
    assert_eq!(bodies.declaration_sha256(), capture.declarations.declaration_sha256());
    assert!(!bodies.belongs_to(&capture::reference().sources));
    assert_eq!(
        verify_bodies(&capture::reference().sources, &capture.declarations)
            .expect_err("different original map")
            .detail(),
        "body-source-map-identity"
    );
    let add = bodies
        .functions()
        .iter()
        .find(|function| function.name() == "add")
        .expect("scalar C export");
    assert_eq!(add.result_type(), ValueType::I32);
    assert_eq!(add.expressions().len(), 3);
    assert!(add.expressions().iter().all(|expression| expression.value_type() == ValueType::I32));
    assert!(matches!(add.steps(), [FlowStep::Return { cleanup, .. }] if cleanup.is_empty()));
    let read = bodies
        .functions()
        .iter()
        .find(|function| function.name() == "readSeed")
        .expect("handle body");
    assert_eq!(read.owner_origins().len(), 1);
    let origin = &read.owner_origins()[0];
    assert_eq!(origin.library(), "fixture-c-v0@0");
    assert_eq!(origin.kind(), "fixture-c-v0@0/fixture_handle");
    assert_eq!(origin.release_key(), "fixture-c-v0@0/fixture_close");
    assert_eq!(origin.output_slots().len(), 1);
}

#[test]
fn native_c_body_v0_required_runtime_actions_are_not_static_runtime_claims() {
    let capture = capture::reference();
    let bodies = verify_bodies(&capture.sources, &capture.declarations).expect("typed plans");
    let sum =
        bodies.functions().iter().find(|function| function.name() == "sum").expect("byte wrapper");
    let utf8 = bodies
        .functions()
        .iter()
        .find(|function| function.name() == "utf8Sum")
        .expect("String wrapper");
    assert!(sum.steps().iter().any(|step| matches!(step,
        FlowStep::PrepareLoan { utf8: false, maximum_bytes: 4096, traps, .. }
        if traps.contains(&TrapRequirement::ForeignLength)
            && traps.contains(&TrapRequirement::ForeignByteRange)
            && traps.contains(&TrapRequirement::PreservePrivatePreparationIdentity)
    )));
    assert!(utf8.steps().iter().any(|step| matches!(step,
        FlowStep::PrepareLoan { utf8: true, maximum_bytes: 4096, traps, .. }
        if traps.contains(&TrapRequirement::ForeignLength)
            && !traps.contains(&TrapRequirement::ForeignByteRange)
    )));
    for function in bodies.functions() {
        for (index, step) in function.steps().iter().enumerate() {
            if let FlowStep::Call { call, .. } = step {
                assert!(matches!(function.steps().get(index - 1), Some(FlowStep::Reserve {
                    call: reservation, live_limit: 64, trap: TrapRequirement::ForeignResourceLimit, ..
                }) if reservation == call));
            }
        }
    }
}
