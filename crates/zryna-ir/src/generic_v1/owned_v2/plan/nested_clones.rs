//! Fixed raw nested CFGs and live-loan replay independent of all source production.
use super::*;
use crate::generic_v1::inventory::Inventory;
use zryna_layout::{StorageTarget, generic_v1 as layout};
use zryna_source::{SourceFileInput, SourceMap, UntrustedSpan};

pub(super) fn definition(id: u32, ty: graph::Type) -> graph::Definition {
    graph::Definition { id, ty }
}
pub(super) fn borrow(referent: u32) -> graph::Type {
    graph::Type::Borrow { referent, exclusive: false }
}
fn span() -> UntrustedSpan {
    UntrustedSpan { file: 0, start: 0, end: 1 }
}
pub(super) fn instruction(
    id: u32,
    ty: graph::Type,
    operation: graph::Operation,
) -> graph::Instruction {
    graph::Instruction { result: definition(id, ty), span: span(), operation }
}
pub(super) fn edge(target: u32, arguments: Vec<u32>) -> graph::Edge {
    graph::Edge { target, arguments }
}
pub(super) fn arm(ordinal: u32, binding: Option<graph::Definition>, target: u32) -> graph::Arm {
    graph::Arm { ordinal, binding, edge: edge(target, vec![]) }
}
pub(super) fn block(
    id: u32,
    parameters: Vec<graph::Definition>,
    instructions: Vec<graph::Instruction>,
    terminator: graph::Terminator,
) -> graph::Block {
    graph::Block { id, parameters, instructions, span: span(), terminator }
}

// Four explicitly specified blocks: borrowed Option selection, none/some, rebuild outer.
pub(super) fn child(
    blocks: &mut Vec<graph::Block>,
    ext: &mut Vec<raw::Extension>,
    start: u32,
    first: u32,
    ordinal: u32,
    outer_join: u32,
) {
    blocks.extend([
        block(
            start,
            vec![definition(first, borrow(3))],
            vec![],
            graph::Terminator::ClosedEnumMatch {
                scrutinee: first,
                ty: 3,
                mode: graph::MatchMode::SharedBorrow,
                arms: vec![
                    arm(0, None, start + 1),
                    arm(1, Some(definition(first + 2, borrow(2))), start + 2),
                ],
            },
        ),
        block(
            start + 1,
            vec![],
            vec![instruction(
                first + 1,
                graph::Type::Stored(3),
                graph::Operation::ClosedEnumConstruct { ty: 3, ordinal: 0, payload: None },
            )],
            graph::Terminator::Jump(edge(start + 3, vec![first + 1])),
        ),
        block(
            start + 2,
            vec![definition(first + 2, borrow(2))],
            vec![
                instruction(first + 3, graph::Type::Stored(2), graph::Operation::Unit),
                instruction(
                    first + 4,
                    graph::Type::Stored(3),
                    graph::Operation::ClosedEnumConstruct {
                        ty: 3,
                        ordinal: 1,
                        payload: Some(first + 3),
                    },
                ),
                instruction(first + 5, graph::Type::Unit, graph::Operation::Unit),
            ],
            graph::Terminator::Jump(edge(start + 3, vec![first + 4])),
        ),
        block(
            start + 3,
            vec![definition(first + 6, graph::Type::Stored(3))],
            vec![
                instruction(
                    first + 7,
                    graph::Type::Stored(4),
                    graph::Operation::ClosedEnumConstruct {
                        ty: 4,
                        ordinal,
                        payload: Some(first + 6),
                    },
                ),
                instruction(first + 8, graph::Type::Unit, graph::Operation::Unit),
            ],
            graph::Terminator::Jump(edge(outer_join, vec![first + 7])),
        ),
    ]);
    ext.extend([
        raw::Extension {
            result: first + 3,
            operation: raw::Operation::CloneBorrowedString(first + 2),
        },
        raw::Extension { result: first + 5, operation: raw::Operation::EndLoan(first + 2) },
        raw::Extension { result: first + 8, operation: raw::Operation::EndLoan(first) },
    ]);
}

fn verified_layouts(result: bool, sources: &SourceMap) -> VerifiedLayouts {
    let kinds = vec![
        layout::raw::TypeKind::Base(zryna_layout::raw::TypeKind::Bool),
        layout::raw::TypeKind::Base(zryna_layout::raw::TypeKind::I32),
        layout::raw::TypeKind::Base(zryna_layout::raw::TypeKind::String),
        layout::raw::TypeKind::Option { argument: layout::raw::NodeId(2) },
        if result {
            layout::raw::TypeKind::Result {
                okay: layout::raw::NodeId(3),
                error: layout::raw::NodeId(3),
            }
        } else {
            layout::raw::TypeKind::Option { argument: layout::raw::NodeId(3) }
        },
    ];
    layout::verify(
        &layout::raw::Graph {
            modules: vec![layout::raw::Module {
                id: layout::raw::ModuleId(0),
                source_file: sources.verify_file_id(0).expect("file"),
                data_declarations: 0,
            }],
            declarations: vec![],
            types: kinds
                .into_iter()
                .enumerate()
                .map(|(id, kind)| layout::raw::TypeNode {
                    id: layout::raw::NodeId(u32::try_from(id).expect("id")),
                    span: None,
                    kind,
                })
                .collect(),
            program_roots: (0..5).map(layout::raw::NodeId).collect(),
        },
        sources,
        StorageTarget::Linear32V1,
    )
    .expect("independent closed layout")
}

fn fixture(result: bool) -> (raw::Program, VerifiedLayouts, SourceMap) {
    let sources =
        SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text: "x".into() }])
            .expect("source");
    let layouts = verified_layouts(result, &sources);
    let outer_join = if result { 9 } else { 6 };
    let arms = if result {
        vec![arm(0, Some(definition(3, borrow(3))), 1), arm(1, Some(definition(12, borrow(3))), 5)]
    } else {
        vec![arm(0, None, 1), arm(1, Some(definition(4, borrow(3))), 2)]
    };
    let mut blocks = vec![block(
        0,
        vec![definition(0, graph::Type::Stored(4))],
        vec![
            instruction(1, borrow(4), graph::Operation::Unit),
            instruction(2, borrow(4), graph::Operation::Unit),
        ],
        graph::Terminator::ClosedEnumMatch {
            scrutinee: 2,
            ty: 4,
            mode: graph::MatchMode::SharedBorrow,
            arms,
        },
    )];
    let mut ext = vec![
        raw::Extension {
            result: 1,
            operation: raw::Operation::Borrow { value: 0, exclusive: false },
        },
        raw::Extension {
            result: 2,
            operation: raw::Operation::Borrow { value: 0, exclusive: false },
        },
    ];
    let output = if result {
        child(&mut blocks, &mut ext, 1, 3, 0, outer_join);
        child(&mut blocks, &mut ext, 5, 12, 1, outer_join);
        21
    } else {
        blocks.push(block(
            1,
            vec![],
            vec![instruction(
                3,
                graph::Type::Stored(4),
                graph::Operation::ClosedEnumConstruct { ty: 4, ordinal: 0, payload: None },
            )],
            graph::Terminator::Jump(edge(outer_join, vec![3])),
        ));
        child(&mut blocks, &mut ext, 2, 4, 1, outer_join);
        13
    };
    blocks.push(block(
        outer_join,
        vec![definition(output, graph::Type::Stored(4))],
        vec![
            instruction(output + 1, graph::Type::Unit, graph::Operation::Unit),
            instruction(output + 2, graph::Type::Unit, graph::Operation::Unit),
            instruction(output + 3, graph::Type::Unit, graph::Operation::Unit),
            instruction(output + 4, graph::Type::Stored(1), graph::Operation::I32Literal(7)),
        ],
        graph::Terminator::Return(output + 4),
    ));
    ext.extend([
        raw::Extension { result: output + 1, operation: raw::Operation::EndLoan(2) },
        raw::Extension { result: output + 2, operation: raw::Operation::Drop(output) },
        raw::Extension { result: output + 3, operation: raw::Operation::EndLoan(1) },
    ]);
    let (mut graph, _, _) = tests::fixture();
    graph.type_keys = layouts.types().map(|t| t.key().to_vec()).collect();
    graph.universe = *layouts.universe_identity();
    graph.linear32 = *layouts.fingerprint();
    graph.functions[0].parameters = vec![graph::Type::Stored(4)];
    graph.functions[0].blocks = blocks;
    (raw::Program { graph, extensions: vec![ext], plans: vec![] }, layouts, sources)
}

pub(super) fn typed(
    claim: &raw::Program,
    layouts: &VerifiedLayouts,
    s: &SourceMap,
) -> Result<(), Failure> {
    super::super::typed::check(
        claim,
        s,
        layouts,
        &Inventory { generic_count: 0, declarations: vec![0] },
    )
}

#[test]
fn independent_nested_selected_paths_retain_root_and_end_deepest_loans_before_cleanup() {
    for result in [false, true] {
        let (claim, layouts, s) = fixture(result);
        typed(&claim, &layouts, &s).expect("fixed independent typed nested CFG");
        let plans = derive(&claim.graph, &claim.extensions, &layouts)
            .expect("independent owner/loan replay");
        let failures = plans[0].steps.iter().filter(|s| s.failure).collect::<Vec<_>>();
        let expected = if result {
            vec![(3, vec![5, 3, 2, 1]), (7, vec![14, 12, 2, 1])]
        } else {
            vec![(4, vec![6, 4, 2, 1])]
        };
        assert_eq!(failures.len(), expected.len());
        for (step, (block, loans)) in failures.iter().zip(expected) {
            assert_eq!(step.block, block);
            assert_eq!(step.position, 0);
            assert_eq!(step.end_loans, loans);
            assert_eq!(step.cleanup, [0]);
        }
        let last = plans[0].steps.last().expect("return");
        assert!(!last.failure);
        assert!(last.end_loans.is_empty());
        assert_eq!(last.cleanup, [0]);
    }
}

#[test]
fn independent_nested_typed_leaf_rejects_outer_inner_and_fresh_owner_operands() {
    for result in [false, true] {
        let (claim, layouts, s) = fixture(result);
        let first = if result { 3 } else { 4 };
        for id in [0, 2, first, first + 3] {
            let mut hostile = claim.clone();
            hostile.extensions[0]
                .iter_mut()
                .find(|e| e.result == first + 3)
                .expect("clone")
                .operation = raw::Operation::CloneBorrowedString(id);
            assert!(typed(&hostile, &layouts, &s).is_err());
            typed(&claim, &layouts, &s).expect("pristine retry");
        }
    }
}

#[test]
fn independent_nested_typing_rejects_forged_family_ordinal_payload_and_binding() {
    for result in [false, true] {
        let (claim, layouts, sources) = fixture(result);
        let (start, first) = if result { (1, 3) } else { (2, 4) };
        for attack in 0..10 {
            let mut hostile = claim.clone();
            let blocks = &mut hostile.graph.functions[0].blocks;
            match attack {
                0 | 1 | 7 | 8 => {
                    let graph::Terminator::ClosedEnumMatch { ty, scrutinee, arms, .. } =
                        &mut blocks[start].terminator
                    else {
                        panic!("inner match")
                    };
                    match attack {
                        0 => *ty = 4,
                        1 => *scrutinee = 2,
                        7 => arms[1].binding = None,
                        _ => arms[1].ordinal = 0,
                    }
                }
                2 => {
                    blocks[start].parameters[0].ty = borrow(2);
                    let graph::Terminator::ClosedEnumMatch { arms, .. } = &mut blocks[0].terminator
                    else {
                        panic!("outer match")
                    };
                    arms[usize::from(!result)].binding.as_mut().expect("payload").ty = borrow(2);
                }
                3 => {
                    blocks[start + 1].instructions[0].operation =
                        graph::Operation::ClosedEnumConstruct {
                            ty: 3,
                            ordinal: 0,
                            payload: Some(0),
                        }
                }
                4 => {
                    blocks[start + 2].instructions[1].operation =
                        graph::Operation::ClosedEnumConstruct {
                            ty: 3,
                            ordinal: 0,
                            payload: Some(first + 3),
                        }
                }
                5 => {
                    blocks[start + 2].instructions[1].operation =
                        graph::Operation::ClosedEnumConstruct {
                            ty: 4,
                            ordinal: 1,
                            payload: Some(first + 3),
                        }
                }
                6 => {
                    blocks[start + 3].instructions[0].operation =
                        graph::Operation::ClosedEnumConstruct {
                            ty: 4,
                            ordinal: u32::from(!result),
                            payload: Some(0),
                        }
                }
                _ => blocks[start + 2].instructions[0].result.ty = graph::Type::Stored(3),
            }
            let Failure::Diagnostics(errors) = typed(&hostile, &layouts, &sources)
                .expect_err("independent nested identity/shape attack")
            else {
                panic!("typed diagnostic")
            };
            assert_eq!(errors[0].code, "ZRYNA-I7001");
            typed(&claim, &layouts, &sources).expect("pristine after every shape mutation");
        }
    }
}

#[test]
fn independent_nested_planner_rejects_exclusive_ended_foreign_loans_and_bad_end_order() {
    for result in [false, true] {
        let (claim, layouts, _) = fixture(result);
        let first = if result { 3 } else { 4 };
        let some = if result { 3 } else { 4 };
        let output = if result { 21 } else { 13 };
        for attack in 0..6 {
            let mut hostile = claim.clone();
            match attack {
                0 => {
                    hostile.extensions[0].retain(|e| e.result != 1 && e.result != output + 3);
                    hostile.graph.functions[0].blocks[0].instructions[0] =
                        instruction(1, graph::Type::Stored(1), graph::Operation::I32Literal(0));
                    hostile.extensions[0][0].operation =
                        raw::Operation::Borrow { value: 0, exclusive: true };
                }
                1 => {
                    hostile.extensions[0]
                        .iter_mut()
                        .find(|e| e.result == first + 3)
                        .expect("clone")
                        .operation = raw::Operation::EndLoan(first + 2);
                    hostile.graph.functions[0].blocks[some].instructions[0].result.ty =
                        graph::Type::Unit;
                    hostile.extensions[0]
                        .iter_mut()
                        .find(|e| e.result == first + 5)
                        .expect("end")
                        .operation = raw::Operation::CloneBorrowedString(first + 2);
                    hostile.graph.functions[0].blocks[some].instructions[2].result.ty =
                        graph::Type::Stored(2);
                }
                2 => {
                    hostile.extensions[0]
                        .iter_mut()
                        .find(|e| e.result == first + 3)
                        .expect("clone")
                        .operation = raw::Operation::CloneBorrowedString(0);
                }
                3 => {
                    hostile.extensions[0]
                        .iter_mut()
                        .find(|e| e.result == first + 5)
                        .expect("end")
                        .operation = raw::Operation::EndLoan(2);
                }
                4 => {
                    hostile.extensions[0]
                        .iter_mut()
                        .find(|e| e.result == first + 5)
                        .expect("end")
                        .operation = raw::Operation::EndLoan(1);
                }
                _ => {
                    hostile.extensions[0]
                        .iter_mut()
                        .find(|e| e.result == output + 2)
                        .expect("drop")
                        .operation = raw::Operation::Drop(0);
                }
            }
            let Failure::Diagnostics(errors) =
                derive(&hostile.graph, &hostile.extensions, &layouts)
                    .expect_err("nested planner attack")
            else {
                panic!("diagnostic")
            };
            assert_eq!(errors[0].code, "ZRYNA-I7001");
            if attack == 0 {
                assert_eq!(errors[0].message, "selected clone requires a live shared String loan");
            }
            derive(&claim.graph, &claim.extensions, &layouts)
                .expect("pristine retry after every hostile replay");
        }
    }
}
