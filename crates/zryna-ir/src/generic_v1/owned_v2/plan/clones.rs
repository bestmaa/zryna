//! Independent typed claims and owner replay; no source producer supplies these graphs.
use super::*;
use crate::generic_v1::inventory::Inventory;
use zryna_source::{SourceFileInput, SourceMap, UntrustedSpan};

fn fixture() -> (raw::Program, VerifiedLayouts, SourceMap) {
    let (mut graph, _, layouts) = tests::fixture();
    let span = UntrustedSpan { file: 0, start: 0, end: 1 };
    let instruction = |id, ty, operation| graph::Instruction {
        result: graph::Definition { id, ty },
        span,
        operation,
    };
    graph.functions[0].blocks = vec![graph::Block {
        id: 0,
        parameters: vec![graph::Definition { id: 0, ty: graph::Type::Stored(2) }],
        instructions: vec![
            instruction(
                1,
                graph::Type::Borrow { referent: 2, exclusive: false },
                graph::Operation::Unit,
            ),
            instruction(2, graph::Type::Stored(2), graph::Operation::Unit),
            instruction(3, graph::Type::Unit, graph::Operation::Unit),
            instruction(4, graph::Type::Unit, graph::Operation::Unit),
            instruction(5, graph::Type::Stored(1), graph::Operation::I32Literal(7)),
        ],
        span,
        terminator: graph::Terminator::Return(5),
    }];
    let extensions = vec![vec![
        raw::Extension {
            result: 1,
            operation: raw::Operation::Borrow { value: 0, exclusive: false },
        },
        raw::Extension { result: 2, operation: raw::Operation::CloneBorrowedString(1) },
        raw::Extension { result: 3, operation: raw::Operation::EndLoan(1) },
        raw::Extension { result: 4, operation: raw::Operation::Drop(2) },
    ]];
    let sources =
        SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text: "x".into() }])
            .expect("source");
    (raw::Program { graph, extensions, plans: vec![] }, layouts, sources)
}

fn typed(
    claim: &raw::Program,
    layouts: &VerifiedLayouts,
    sources: &SourceMap,
) -> Result<(), Failure> {
    super::super::typed::check(
        claim,
        sources,
        layouts,
        &Inventory { generic_count: 0, declarations: vec![0] },
    )
}

#[test]
fn independent_selected_clone_failure_precedes_fresh_owner_registration() {
    let (claim, layouts, sources) = fixture();
    typed(&claim, &layouts, &sources).expect("independent typed claim");
    let plans =
        derive(&claim.graph, &claim.extensions, &layouts).expect("independent live shared loan");
    assert_eq!(
        plans[0].steps,
        vec![
            raw::Step {
                block: 0,
                position: 1,
                end_loans: vec![1],
                cleanup: vec![0],
                failure: true
            },
            raw::Step {
                block: 0,
                position: 5,
                end_loans: vec![],
                cleanup: vec![0],
                failure: false
            },
        ]
    );
}

#[test]
fn independent_typed_selected_clone_rejects_wrong_operand_result_and_placeholder() {
    let (claim, layouts, sources) = fixture();
    for attack in 0..7 {
        let mut hostile = claim.clone();
        match attack {
            0 => hostile.extensions[0][1].operation = raw::Operation::CloneBorrowedString(0),
            1 => {
                hostile.extensions[0][0].operation =
                    raw::Operation::Borrow { value: 0, exclusive: true };
                hostile.graph.functions[0].blocks[0].instructions[0].result.ty =
                    graph::Type::Borrow { referent: 2, exclusive: true }
            }
            2 => {
                hostile.graph.functions[0].parameters[0] = graph::Type::Stored(0);
                hostile.graph.functions[0].blocks[0].parameters[0].ty = graph::Type::Stored(0);
                hostile.graph.functions[0].blocks[0].instructions[0].result.ty =
                    graph::Type::Borrow { referent: 0, exclusive: false }
            }
            3 => {
                hostile.graph.functions[0].blocks[0].instructions[1].result.ty =
                    graph::Type::Stored(0);
            }
            4 => {
                hostile.graph.functions[0].blocks[0].instructions[1].operation =
                    graph::Operation::I32Literal(0);
            }
            5 => hostile.extensions[0][1].operation = raw::Operation::CloneBorrowedString(u32::MAX),
            _ => hostile.extensions[0].push(raw::Extension {
                result: 99,
                operation: raw::Operation::CloneBorrowedString(1),
            }),
        }
        let Failure::Diagnostics(errors) =
            typed(&hostile, &layouts, &sources).expect_err("independent typed attack")
        else {
            panic!("typed diagnostic")
        };
        if matches!(attack, 1 | 2) {
            assert_eq!(errors[0].message, "selected clone requires exact shared String loan");
        }
        typed(&claim, &layouts, &sources).expect("pristine typed retry");
    }
}

#[test]
fn independent_planner_selected_clone_requires_actual_live_shared_loan() {
    let (claim, layouts, _) = fixture();
    for attack in 0..5 {
        let mut hostile = claim.clone();
        match attack {
            0 => hostile.extensions[0][1].operation = raw::Operation::CloneBorrowedString(0),
            1 => {
                hostile.extensions[0][0].operation =
                    raw::Operation::Borrow { value: 0, exclusive: true }
            }
            2 => {
                hostile.extensions[0][0].operation =
                    raw::Operation::Borrow { value: 0, exclusive: true };
                hostile.graph.functions[0].blocks[0].instructions[0].result.ty =
                    graph::Type::Borrow { referent: 2, exclusive: true };
            }
            3 => {
                hostile.extensions[0][1].operation = raw::Operation::EndLoan(1);
                hostile.graph.functions[0].blocks[0].instructions[1].result.ty = graph::Type::Unit;
                hostile.extensions[0][2].operation = raw::Operation::CloneBorrowedString(1);
                hostile.graph.functions[0].blocks[0].instructions[2].result.ty =
                    graph::Type::Stored(2);
            }
            _ => hostile.extensions[0][1].operation = raw::Operation::CloneBorrowedString(u32::MAX),
        }
        let Failure::Diagnostics(errors) = derive(&hostile.graph, &hostile.extensions, &layouts)
            .expect_err("independent planner attack")
        else {
            panic!("diagnostic")
        };
        assert_eq!(errors[0].code, "ZRYNA-I7001");
        derive(&claim.graph, &claim.extensions, &layouts).expect("pristine planner retry");
    }
}
