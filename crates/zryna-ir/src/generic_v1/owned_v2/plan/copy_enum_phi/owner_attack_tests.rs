//! Independent owner-state agreement and ordinary-loan edge rejection.
use super::base_fixture_tests::{expected_plan, fixture};
use super::typed_attack_tests::diag;
use super::*;
use zryna_source::UntrustedSpan;
#[test]
fn independent_copy_enum_replay_rejects_leaked_substituted_and_missing_root_states() {
    let (p, linear, linux, sources) = fixture();
    let expected = expected_plan();
    for attack in 0..6 {
        let mut h = p.clone();
        match attack {
            0 => {
                h.extensions[0].remove(1);
            } // Live owner6 versus root-only state.
            1 => {
                h.extensions[0].remove(3);
            } // Live owner11 versus root-only state.
            2 => h.extensions[0][1].operation = raw::Operation::Drop(0),
            3 => h.extensions[0][3].operation = raw::Operation::Drop(0),
            4 => {
                h.extensions[0].retain(|e| e.result != 10 && e.result != 12);
            } // Different live owner identities despite equal String types.
            _ => {
                h.extensions[0][0].operation = raw::Operation::Move(0);
                h.extensions[0][2].operation = raw::Operation::Move(0);
            } // Both arms consume/drop original root; common exit uses unavailable root.
        }
        // Missing extension placeholders remain legal Unit. State replay owns these rejections.
        typed(&h, &linear, &linux, &sources).expect("type-valid ownership attack");
        for layouts in [&linear, &linux] {
            diag(derive(&h.graph, &h.extensions, layouts));
        }
        assert_eq!(
            derive(&p.graph, &p.extensions, &linear).expect("pristine ownership recovery"),
            expected
        );
    }
}

#[test]
fn independent_ordinary_edge_cannot_transport_a_loan_even_with_exact_borrow_types() {
    let (_, linear, linux, sources) = fixture();
    let at = UntrustedSpan { file: 0, start: 0, end: 1 };
    let loan = graph::Type::Borrow { referent: 2, exclusive: false };
    let instruction =
        |id, ty, operation| graph::Instruction { result: d(id, ty), span: at, operation };
    let edge = |target, args| graph::Edge { target, arguments: args };
    let blocks = vec![
        graph::Block {
            id: 0,
            parameters: vec![d(0, graph::Type::Stored(2)), d(1, graph::Type::Stored(0))],
            instructions: vec![instruction(2, loan, graph::Operation::Unit)],
            span: at,
            terminator: graph::Terminator::Branch {
                condition: 1,
                yes: edge(1, vec![]),
                no: edge(2, vec![]),
            },
        },
        graph::Block {
            id: 1,
            parameters: vec![],
            instructions: vec![],
            span: at,
            terminator: graph::Terminator::Jump(edge(3, vec![2])),
        },
        graph::Block {
            id: 2,
            parameters: vec![],
            instructions: vec![],
            span: at,
            terminator: graph::Terminator::Jump(edge(3, vec![2])),
        },
        graph::Block {
            id: 3,
            parameters: vec![d(3, loan)],
            instructions: vec![
                instruction(4, graph::Type::Unit, graph::Operation::Unit),
                instruction(5, graph::Type::Unit, graph::Operation::Unit),
                instruction(6, graph::Type::Stored(1), graph::Operation::I32Literal(7)),
            ],
            span: at,
            terminator: graph::Terminator::Return(6),
        },
    ];
    let extensions = vec![
        raw::Extension {
            result: 2,
            operation: raw::Operation::Borrow { value: 0, exclusive: false },
        },
        raw::Extension { result: 4, operation: raw::Operation::EndLoan(2) },
        raw::Extension { result: 5, operation: raw::Operation::Drop(0) },
    ];
    let p = claim(blocks, extensions, &linear, &linux);
    typed(&p, &linear, &linux, &sources)
        .expect("exact borrow types alone do not grant ownership edge admission");
    for layouts in [&linear, &linux] {
        diag(derive(&p.graph, &p.extensions, layouts));
    }
    let (pristine, linear, linux, sources) = fixture();
    typed(&pristine, &linear, &linux, &sources).expect("fresh pristine recovery");
    assert_eq!(
        derive(&pristine.graph, &pristine.extensions, &linear)
            .expect("loan attack cannot poison later replay"),
        expected_plan()
    );
}
