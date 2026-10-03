//! Independent malformed inventory, exact calls, payload and dominance fixtures.

use super::super::{raw, validate_closed_graph};
use super::fixtures::{Fixture, definition, key, rejection};
use zryna_source::{SourceFileInput, SourceMap};

#[test]
fn complete_option_match_and_distinct_generic_calls_pass_only_structural_validation() {
    Fixture::enum_match().check().expect("exact typed match");
    Fixture::generic_calls().check().expect("two exact instances, one original declaration");
}

#[test]
fn foreign_layout_source_target_and_changed_complete_inventory_reject_then_replay() {
    let fixture = Fixture::enum_match();
    let equal = SourceMap::build(vec![SourceFileInput {
        path: "main.zry".into(),
        text: "//π\r\nfunction f(): i32 { return 7; }".into(),
    }])
    .expect("separate compilation");
    rejection(
        validate_closed_graph(&fixture.program, &equal, &fixture.linear, &fixture.linux),
        "ZRYNA-I7001",
    );
    rejection(
        validate_closed_graph(&fixture.program, &fixture.sources, &fixture.linux, &fixture.linear),
        "ZRYNA-I7001",
    );
    for case in 0..7 {
        let mut program = fixture.program.clone();
        match case {
            0 => program.universe[0] ^= 1,
            1 => program.linear32[0] ^= 1,
            2 => program.linux_x86_64[0] ^= 1,
            3 => {
                program.type_keys.pop();
            }
            4 => {
                program.type_keys.push(vec![1]);
            }
            5 => program.type_keys.swap(0, 1),
            _ => program.type_keys[3][0] = 0x40,
        }
        rejection(
            validate_closed_graph(&program, &fixture.sources, &fixture.linear, &fixture.linux),
            "ZRYNA-I7001",
        );
    }
    fixture.check().expect("pristine replay");
}

#[test]
fn forged_function_key_index_arity_signature_export_or_omitted_root_rejects() {
    for case in 0..9 {
        let mut fixture = Fixture::generic_calls();
        match case {
            0 => fixture.program.functions[0].key[0] = 0x41,
            1 => fixture.program.functions.swap(0, 1),
            2 => fixture.program.functions[1].key = fixture.program.functions[0].key.clone(),
            3 => fixture.program.functions[0].key = key(0x40, 0, 99, &[&[0]]),
            4 => fixture.program.declarations[0].parameters = 2,
            5 => fixture.program.functions[0].parameters[0] = raw::Type::Stored(1),
            6 => fixture.program.functions[0].public_export = Some("forged".into()),
            7 => {
                fixture.program.functions.pop();
            }
            _ => fixture.program.declarations[0].parameters = 3,
        }
        rejection(fixture.check(), "ZRYNA-I7001");
    }
    Fixture::generic_calls().check().expect("pristine replay");
}

#[test]
fn forged_generic_call_target_arity_type_and_forward_operand_reject() {
    for case in 0..5 {
        let mut fixture = Fixture::generic_calls();
        let operation = &mut fixture.program.functions[2].blocks[0].instructions[1].operation;
        *operation = match case {
            0 => raw::Operation::ClosedGenericCall { instance: 2, arguments: vec![0] },
            1 => raw::Operation::ClosedGenericCall { instance: 1, arguments: vec![] },
            2 => raw::Operation::ClosedGenericCall { instance: 0, arguments: vec![0] },
            3 => raw::Operation::ClosedGenericCall { instance: 1, arguments: vec![1] },
            _ => raw::Operation::SourceCall { module: 0, function: 0, arguments: vec![0] },
        };
        rejection(fixture.check(), "ZRYNA-I7001");
    }
}

#[test]
fn distinct_keys_of_one_original_function_cannot_hide_source_recursion() {
    let mut fixture = Fixture::generic_calls();
    let span = fixture.program.functions[0].span;
    fixture.program.functions[0].blocks[0].instructions = vec![
        raw::Instruction {
            result: definition(1, raw::Type::Stored(1)),
            span,
            operation: raw::Operation::I32Literal(0),
        },
        raw::Instruction {
            result: definition(2, raw::Type::Stored(1)),
            span,
            operation: raw::Operation::ClosedGenericCall { instance: 1, arguments: vec![1] },
        },
    ];
    rejection(fixture.check(), "ZRYNA-I7001");
}

#[test]
fn unknown_ordinal_wrong_family_wrong_payload_type_and_presence_reject() {
    for case in 0..6 {
        let mut fixture = Fixture::enum_match();
        fixture.program.functions[0].blocks[0].instructions[1].operation = match case {
            0 => raw::Operation::ClosedEnumConstruct { ty: 3, ordinal: 2, payload: Some(0) },
            1 => raw::Operation::ClosedEnumConstruct { ty: 1, ordinal: 0, payload: None },
            2 => raw::Operation::ClosedEnumConstruct { ty: 3, ordinal: 0, payload: Some(0) },
            3 => raw::Operation::ClosedEnumConstruct { ty: 3, ordinal: 1, payload: None },
            4 => raw::Operation::ClosedEnumConstruct { ty: 3, ordinal: 1, payload: Some(1) },
            _ => raw::Operation::ClosedEnumConstruct { ty: 99, ordinal: 0, payload: None },
        };
        rejection(fixture.check(), "ZRYNA-I7001");
    }
}

#[test]
fn missing_duplicate_reordered_or_wrong_payload_binding_and_loan_mode_reject() {
    for case in 0..7 {
        let mut fixture = Fixture::enum_match();
        let raw::Terminator::ClosedEnumMatch { arms, mode, .. } =
            &mut fixture.program.functions[0].blocks[0].terminator
        else {
            panic!("fixture")
        };
        match case {
            0 => {
                arms.pop();
            }
            1 => arms[1].ordinal = 0,
            2 => arms.swap(0, 1),
            3 => arms[1].binding = None,
            4 => arms[1].binding = Some(definition(3, raw::Type::Stored(0))),
            5 => arms[0].binding = Some(definition(2, raw::Type::Stored(1))),
            _ => *mode = raw::MatchMode::SharedBorrow,
        }
        rejection(fixture.check(), "ZRYNA-I7001");
    }
}

#[test]
fn inactive_payload_cannot_dominate_another_arm_or_hide_in_an_unreachable_block() {
    let mut fixture = Fixture::enum_match();
    fixture.program.functions[0].blocks[1].terminator = raw::Terminator::Return(3);
    rejection(fixture.check(), "ZRYNA-I7001");
    let mut fixture = Fixture::enum_match();
    if let raw::Terminator::ClosedEnumMatch { arms, .. } =
        &mut fixture.program.functions[0].blocks[0].terminator
    {
        arms[1].edge.target = 1;
    }
    rejection(fixture.check(), "ZRYNA-I7001");
}

#[test]
fn dense_ids_unknown_values_entry_edges_and_non_utf8_source_ranges_reject() {
    for case in 0..5 {
        let mut fixture = Fixture::enum_match();
        match case {
            0 => fixture.program.functions[0].blocks[1].id = 2,
            1 => fixture.program.functions[0].blocks[1].instructions[0].result.id = 99,
            2 => fixture.program.functions[0].blocks[1].terminator = raw::Terminator::Return(99),
            3 => {
                fixture.program.functions[0].blocks[1].terminator =
                    raw::Terminator::Jump(raw::Edge { target: 0, arguments: vec![] });
            }
            _ => fixture.program.functions[0].blocks[0].instructions[0].span.start = 3,
        }
        rejection(fixture.check(), "ZRYNA-I7001");
    }
    Fixture::enum_match().check().expect("recovery");
}

#[test]
fn both_option_and_result_ordinals_have_exact_payload_types_without_a_trap_channel() {
    for result in [false, true] {
        for ordinal in 0..2 {
            let mut f = Fixture::new(result);
            let span = f.program.functions[0].span;
            let (ty, operation, payload) = if result && ordinal == 1 {
                (raw::Type::Stored(0), raw::Operation::BoolLiteral(true), Some(0))
            } else {
                (
                    raw::Type::Stored(1),
                    raw::Operation::I32Literal(7),
                    if !result && ordinal == 0 { None } else { Some(0) },
                )
            };
            f.program.functions[0].result = raw::Type::Stored(3);
            f.program.functions[0].blocks[0].instructions = vec![
                raw::Instruction { result: definition(0, ty), span, operation },
                raw::Instruction {
                    result: definition(1, raw::Type::Stored(3)),
                    span,
                    operation: raw::Operation::ClosedEnumConstruct { ty: 3, ordinal, payload },
                },
            ];
            f.program.functions[0].blocks[0].terminator = raw::Terminator::Return(1);
            f.check().expect("each family ordinal is an ordinary exact typed value");
        }
    }
}

#[test]
fn shared_and_exclusive_match_bind_exact_borrowed_payloads_and_reject_mutability_changes() {
    for exclusive in [false, true] {
        let mut f = Fixture::enum_match();
        let span = f.program.functions[0].span;
        let loan = raw::Type::Borrow { referent: 3, exclusive };
        let payload = raw::Type::Borrow { referent: 1, exclusive };
        f.program.functions[0].parameters = vec![loan];
        f.program.functions[0].blocks[0].parameters = vec![definition(0, loan)];
        f.program.functions[0].blocks[0].instructions.clear();
        if let raw::Terminator::ClosedEnumMatch { scrutinee, mode, arms, .. } =
            &mut f.program.functions[0].blocks[0].terminator
        {
            *scrutinee = 0;
            *mode = if exclusive {
                raw::MatchMode::ExclusiveBorrow
            } else {
                raw::MatchMode::SharedBorrow
            };
            arms[1].binding = Some(definition(2, payload));
        }
        f.program.functions[0].blocks[1].instructions[0].result.id = 1;
        f.program.functions[0].blocks[1].terminator = raw::Terminator::Return(1);
        f.program.functions[0].blocks[2].parameters = vec![definition(2, payload)];
        f.program.functions[0].blocks[2].instructions = vec![raw::Instruction {
            result: definition(3, raw::Type::Stored(1)),
            span,
            operation: raw::Operation::I32Literal(1),
        }];
        f.check().expect("typed borrowed edges only; loan lifetime remains a separate proof");
        if let raw::Terminator::ClosedEnumMatch { mode, .. } =
            &mut f.program.functions[0].blocks[0].terminator
        {
            *mode = if exclusive {
                raw::MatchMode::SharedBorrow
            } else {
                raw::MatchMode::ExclusiveBorrow
            };
        }
        rejection(f.check(), "ZRYNA-I7001");
    }
}

#[test]
fn reachable_but_irreducible_cycle_with_two_entries_rejects() {
    let mut f = Fixture::new(false);
    let span = f.program.functions[0].span;
    let edge = |target| raw::Edge { target, arguments: vec![] };
    f.program.functions[0].parameters = vec![raw::Type::Stored(0)];
    f.program.functions[0].blocks = vec![
        raw::Block {
            id: 0,
            parameters: vec![definition(0, raw::Type::Stored(0))],
            instructions: vec![],
            span,
            terminator: raw::Terminator::Branch { condition: 0, yes: edge(1), no: edge(2) },
        },
        raw::Block {
            id: 1,
            parameters: vec![],
            instructions: vec![],
            span,
            terminator: raw::Terminator::Jump(edge(2)),
        },
        raw::Block {
            id: 2,
            parameters: vec![],
            instructions: vec![],
            span,
            terminator: raw::Terminator::Branch { condition: 0, yes: edge(1), no: edge(3) },
        },
        raw::Block {
            id: 3,
            parameters: vec![],
            instructions: vec![raw::Instruction {
                result: definition(1, raw::Type::Stored(1)),
                span,
                operation: raw::Operation::I32Literal(0),
            }],
            span,
            terminator: raw::Terminator::Return(1),
        },
    ];
    rejection(f.check(), "ZRYNA-I7001");
}
