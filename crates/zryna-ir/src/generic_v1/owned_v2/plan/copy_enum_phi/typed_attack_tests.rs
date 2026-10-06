//! Independent hostile hand-authored claims; source authentication stays separate.
use super::base_fixture_tests::{expected_plan, fixture};
use super::*;

pub(super) fn diag<T: std::fmt::Debug>(result: Result<T, Failure>) {
    let Failure::Diagnostics(errors) = result.expect_err("independent malformed claim must reject")
    else {
        panic!("expected diagnostic, not allocation failure")
    };
    assert!(!errors.is_empty());
    assert_eq!(errors[0].code, "ZRYNA-I7001");
}

#[test]
fn independent_copy_enum_raw_typing_rejects_structural_payload_and_authority_attacks() {
    let (p, linear, linux, sources) = fixture();
    typed(&p, &linear, &linux, &sources).expect("pristine before attacks");
    for attack in 0..38 {
        let mut h = p.clone();
        match attack {
            0..=14 => structural_attack(&mut h, attack),
            15..=21 => constructor_attack(&mut h, attack),
            22..=28 => match_attack(&mut h, attack),
            _ => authority_attack(&mut h, attack),
        }
        diag(typed(&h, &linear, &linux, &sources));
        typed(&p, &linear, &linux, &sources)
            .expect("fresh pristine typed recovery after each attack");
        assert_eq!(
            derive(&p.graph, &p.extensions, &linear).expect("fresh pristine plan recovery"),
            expected_plan()
        );
    }
}

fn structural_attack(h: &mut raw::Program, attack: u32) {
    let blocks = &mut h.graph.functions[0].blocks;
    match attack {
        0 => {
            let graph::Terminator::Jump(e) = &mut blocks[1].terminator else { panic!("jump") };
            e.arguments.pop();
        }
        1 => {
            let graph::Terminator::Jump(e) = &mut blocks[1].terminator else { panic!("jump") };
            e.arguments.push(7);
        }
        2 => {
            let graph::Terminator::Jump(e) = &mut blocks[1].terminator else { panic!("jump") };
            e.arguments.swap(0, 1);
        }
        3 => {
            let graph::Terminator::Jump(e) = &mut blocks[1].terminator else { panic!("jump") };
            e.arguments[0] = 2;
        }
        4 => {
            let graph::Terminator::Jump(e) = &mut blocks[1].terminator else { panic!("jump") };
            e.arguments[1] = 1;
        }
        5 => blocks[3].parameters[0].ty = graph::Type::Stored(RESULT),
        6 => blocks[3].parameters[0].ty = graph::Type::Stored(u32::MAX),
        7 => {
            let graph::Terminator::Jump(e) = &mut blocks[1].terminator else { panic!("jump") };
            e.arguments[0] = u32::MAX;
        }
        8 => {
            let graph::Terminator::Jump(e) = &mut blocks[2].terminator else { panic!("jump") };
            e.arguments[0] = 7;
        } // Same-typed sibling-arm value.
        9 => {
            let graph::Terminator::Jump(e) = &mut blocks[1].terminator else { panic!("jump") };
            e.arguments[0] = 13;
        } // Future join parameter.
        10 => blocks[1].instructions[1].result.id = 6,
        11 => blocks[3].parameters[1].id = 13,
        12 => blocks[2].id = 1,
        13 => {
            let graph::Terminator::Jump(e) = &mut blocks[1].terminator else { panic!("jump") };
            e.target = 0;
        }
        14 => {
            let graph::Terminator::Jump(e) = &mut blocks[1].terminator else { panic!("jump") };
            e.target = u32::MAX;
        }
        _ => panic!("foreign attack category"),
    }
}

fn constructor_attack(h: &mut raw::Program, attack: u32) {
    let blocks = &mut h.graph.functions[0].blocks;
    match attack {
        15 => {
            let graph::Operation::ClosedEnumConstruct { payload, .. } =
                &mut blocks[1].instructions[1].operation
            else {
                panic!("None")
            };
            *payload = Some(2);
        }
        16 => {
            let graph::Operation::ClosedEnumConstruct { payload, .. } =
                &mut blocks[0].instructions[1].operation
            else {
                panic!("Some")
            };
            *payload = None;
        }
        17 => {
            let graph::Operation::ClosedEnumConstruct { payload, .. } =
                &mut blocks[0].instructions[1].operation
            else {
                panic!("Some")
            };
            *payload = Some(1);
        }
        18 => {
            let graph::Operation::ClosedEnumConstruct { payload, .. } =
                &mut blocks[1].instructions[3].operation
            else {
                panic!("err")
            };
            *payload = Some(2);
        }
        19 => {
            let graph::Operation::ClosedEnumConstruct { payload, .. } =
                &mut blocks[0].instructions[3].operation
            else {
                panic!("ok")
            };
            *payload = Some(1);
        }
        20 => {
            let graph::Operation::ClosedEnumConstruct { ordinal, .. } =
                &mut blocks[1].instructions[1].operation
            else {
                panic!("construct")
            };
            *ordinal = 2;
        }
        21 => {
            let graph::Operation::ClosedEnumConstruct { ty, .. } =
                &mut blocks[1].instructions[1].operation
            else {
                panic!("construct")
            };
            *ty = 1;
        }
        _ => panic!("foreign attack category"),
    }
}

fn match_attack(h: &mut raw::Program, attack: u32) {
    let blocks = &mut h.graph.functions[0].blocks;
    match attack {
        22 => {
            let graph::Terminator::ClosedEnumMatch { arms, .. } = &mut blocks[3].terminator else {
                panic!("match")
            };
            arms[1].ordinal = 0;
        }
        23 => {
            let graph::Terminator::ClosedEnumMatch { arms, .. } = &mut blocks[6].terminator else {
                panic!("match")
            };
            arms.pop();
        }
        24 => {
            let graph::Terminator::ClosedEnumMatch { arms, .. } = &mut blocks[6].terminator else {
                panic!("match")
            };
            arms[0].binding.as_mut().expect("ok binding").ty = graph::Type::Stored(0);
            blocks[7].parameters[0].ty = graph::Type::Stored(0);
        }
        25 => {
            let graph::Terminator::ClosedEnumMatch { arms, .. } = &mut blocks[6].terminator else {
                panic!("match")
            };
            arms[1].binding.as_mut().expect("err binding").ty = graph::Type::Stored(1);
            blocks[8].parameters[0].ty = graph::Type::Stored(1);
        }
        26 => {
            let graph::Terminator::ClosedEnumMatch { arms, .. } = &mut blocks[3].terminator else {
                panic!("match")
            };
            arms[0].binding = Some(d(15, graph::Type::Stored(1)));
        }
        27 => {
            let graph::Terminator::ClosedEnumMatch { mode, .. } = &mut blocks[3].terminator else {
                panic!("match")
            };
            *mode = graph::MatchMode::SharedBorrow;
        }
        28 => {
            let graph::Terminator::ClosedEnumMatch { scrutinee, .. } = &mut blocks[3].terminator
            else {
                panic!("match")
            };
            *scrutinee = 14;
        }
        _ => panic!("foreign attack category"),
    }
}

fn authority_attack(h: &mut raw::Program, attack: u32) {
    let blocks = &mut h.graph.functions[0].blocks;
    match attack {
        29 => blocks[3].span.file = 1,
        30 => blocks[3].span.end = u32::MAX,
        31 => h.graph.type_keys[OPTION as usize][9] = 0,
        32 => h.graph.linear32[0] ^= 1,
        33 => h.graph.linux_x86_64[0] ^= 1,
        34 => h.graph.universe[0] ^= 1,
        35 => h.graph.type_keys.swap(3, 4),
        36 => h.extensions[0][4].operation = raw::Operation::Drop(13),
        _ => h.extensions[0][4].result = u32::MAX,
    }
}
