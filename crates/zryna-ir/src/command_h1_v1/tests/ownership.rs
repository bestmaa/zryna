use super::{Fixture, raw, reject};

#[test]
fn missing_invented_and_duplicated_source_match_sites_reject() {
    let fixture = Fixture::new();
    let mut missing = fixture.seed();
    missing.modules[0].functions[0].blocks[0].terminators[0].kind =
        raw::Terminator::Jump(raw::Edge { target: raw::BlockId(1), arguments: vec![] });
    reject(&fixture, missing, "ZRYNA-I4100");
    let mut invented = fixture.seed();
    invented.modules[0].functions[0].blocks[0].terminators[0].span = fixture.call;
    reject(&fixture, invented, "ZRYNA-I4100");
    let mut duplicate = fixture.seed();
    let matched = duplicate.modules[0].functions[0].blocks[0].terminators[0].clone();
    duplicate.modules[0].functions[0].blocks[1].terminators.push(matched);
    reject(&fixture, duplicate, "ZRYNA-I4100");
}

#[test]
fn outcome_match_rejects_missing_duplicate_invented_and_inactive_variants() {
    let fixture = Fixture::new();
    for case in 0..4 {
        let mut program = fixture.seed();
        let raw::Terminator::EnumMatch { arms, .. } =
            &mut program.modules[0].functions[0].blocks[0].terminators[0].kind
        else {
            unreachable!()
        };
        match case {
            0 => {
                arms.pop();
            }
            1 => arms[1].variant = 0,
            2 => arms[1].variant = 2,
            3 => {
                arms[0].edge.target = raw::BlockId(2);
                arms[1].edge.target = raw::BlockId(1);
            }
            _ => unreachable!(),
        }
        if case == 0 {
            program.modules[0].functions[0].blocks[1].terminators[0].kind =
                raw::Terminator::Jump(raw::Edge { target: raw::BlockId(2), arguments: vec![] });
            let function = &mut program.modules[0].functions[0];
            function.cleanup_plans.remove(1);
            function.cleanup_plans[1].id = raw::CleanupPlanId(1);
            let raw::Terminator::Return { cleanup, .. } =
                &mut function.blocks[2].terminators[0].kind
            else {
                unreachable!()
            };
            *cleanup = raw::CleanupPlanId(1);
        }
        reject(&fixture, program, if case == 3 { "ZRYNA-I3013" } else { "ZRYNA-I3014" });
    }
}

#[test]
fn payload_cleanup_rejects_leaks_repeated_drop_and_forged_exit_actions() {
    let fixture = Fixture::new();
    let mut leaked = fixture.seed();
    leaked.modules[0].functions[0].blocks[1].instructions.remove(1);
    reject(&fixture, leaked, "ZRYNA-I3012");
    let mut repeated = fixture.seed();
    let drop = repeated.modules[0].functions[0].blocks[1].instructions[1].clone();
    repeated.modules[0].functions[0].blocks[1].instructions.insert(2, drop);
    reject(&fixture, repeated, "ZRYNA-I3010");
    let mut cleanup = fixture.seed();
    cleanup.modules[0].functions[0].cleanup_plans[1].actions =
        vec![raw::DropAction::DropPlace(raw::PlaceId(0))];
    reject(&fixture, cleanup, "ZRYNA-I3012");
    let mut prepare = fixture.seed();
    prepare.modules[0].functions[0].cleanup_plans[0].actions =
        vec![raw::DropAction::DropPlace(raw::PlaceId(0))];
    reject(&fixture, prepare, "ZRYNA-I3012");
}
