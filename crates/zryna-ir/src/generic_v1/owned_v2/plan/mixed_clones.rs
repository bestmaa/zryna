//! Fixed heterogeneous CFGs use exact ordinal payload types, independently of source lowering.
use super::nested_clones::{arm, block, borrow, child, definition, edge, instruction, typed};
use super::*;
use zryna_layout::{StorageTarget, generic_v1 as layout};
use zryna_source::{SourceFileInput, SourceMap};

fn verified_layouts(option_first: bool, sources: &SourceMap) -> VerifiedLayouts {
    let kinds = vec![
        layout::raw::TypeKind::Base(zryna_layout::raw::TypeKind::Bool),
        layout::raw::TypeKind::Base(zryna_layout::raw::TypeKind::I32),
        layout::raw::TypeKind::Base(zryna_layout::raw::TypeKind::String),
        layout::raw::TypeKind::Option { argument: layout::raw::NodeId(2) },
        layout::raw::TypeKind::Result {
            okay: layout::raw::NodeId(if option_first { 3 } else { 2 }),
            error: layout::raw::NodeId(if option_first { 2 } else { 3 }),
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
    .expect("fixed mixed layout")
}

fn fixture(option_first: bool) -> (raw::Program, VerifiedLayouts, SourceMap) {
    let sources =
        SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text: "x".into() }])
            .expect("source");
    let layouts = verified_layouts(option_first, &sources);
    let (nested_start, nested_first, direct_start, direct_first, nested_ordinal) =
        if option_first { (1, 3, 5, 12, 0) } else { (2, 7, 1, 3, 1) };
    let nested_arm = arm(nested_ordinal, Some(definition(nested_first, borrow(3))), nested_start);
    let direct_arm =
        arm(1 - nested_ordinal, Some(definition(direct_first, borrow(2))), direct_start);
    let arms =
        if option_first { vec![nested_arm, direct_arm] } else { vec![direct_arm, nested_arm] };
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
    let direct = block(
        direct_start,
        vec![definition(direct_first, borrow(2))],
        vec![
            instruction(direct_first + 1, graph::Type::Stored(2), graph::Operation::Unit),
            instruction(
                direct_first + 2,
                graph::Type::Stored(4),
                graph::Operation::ClosedEnumConstruct {
                    ty: 4,
                    ordinal: 1 - nested_ordinal,
                    payload: Some(direct_first + 1),
                },
            ),
            instruction(direct_first + 3, graph::Type::Unit, graph::Operation::Unit),
        ],
        graph::Terminator::Jump(edge(6, vec![direct_first + 2])),
    );
    if option_first {
        child(&mut blocks, &mut ext, nested_start, nested_first, nested_ordinal, 6);
        blocks.push(direct);
    } else {
        blocks.push(direct);
        child(&mut blocks, &mut ext, nested_start, nested_first, nested_ordinal, 6);
    }
    ext.extend([
        raw::Extension {
            result: direct_first + 1,
            operation: raw::Operation::CloneBorrowedString(direct_first),
        },
        raw::Extension {
            result: direct_first + 3,
            operation: raw::Operation::EndLoan(direct_first),
        },
    ]);
    blocks.push(block(
        6,
        vec![definition(16, graph::Type::Stored(4))],
        vec![
            instruction(17, graph::Type::Unit, graph::Operation::Unit),
            instruction(18, graph::Type::Unit, graph::Operation::Unit),
            instruction(19, graph::Type::Unit, graph::Operation::Unit),
            instruction(20, graph::Type::Stored(1), graph::Operation::I32Literal(7)),
        ],
        graph::Terminator::Return(20),
    ));
    ext.extend([
        raw::Extension { result: 17, operation: raw::Operation::EndLoan(2) },
        raw::Extension { result: 18, operation: raw::Operation::Drop(16) },
        raw::Extension { result: 19, operation: raw::Operation::EndLoan(1) },
    ]);
    ext.sort_by_key(|e| e.result);
    let (mut graph, _, _) = tests::fixture();
    graph.type_keys = layouts.types().map(|t| t.key().to_vec()).collect();
    graph.universe = *layouts.universe_identity();
    graph.linear32 = *layouts.fingerprint();
    graph.functions[0].parameters = vec![graph::Type::Stored(4)];
    graph.functions[0].blocks = blocks;
    (raw::Program { graph, extensions: vec![ext], plans: vec![] }, layouts, sources)
}

#[test]
fn independent_asymmetric_nested_and_direct_leaf_failure_loans_cleanup_and_root_retention() {
    for option_first in [true, false] {
        let (claim, layouts, s) = fixture(option_first);
        typed(&claim, &layouts, &s).expect("independent ordinal-specific fixed CFG");
        let plans = derive(&claim.graph, &claim.extensions, &layouts).expect("fixed owner replay");
        let failures = plans[0].steps.iter().filter(|s| s.failure).collect::<Vec<_>>();
        let expected = if option_first {
            vec![(3, vec![5, 3, 2, 1]), (5, vec![12, 2, 1])]
        } else {
            vec![(1, vec![3, 2, 1]), (4, vec![9, 7, 2, 1])]
        };
        assert_eq!(failures.len(), 2);
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
fn independent_asymmetric_typing_rejects_swapped_payload_binding_construction_and_leaf() {
    for option_first in [true, false] {
        let (claim, layouts, s) = fixture(option_first);
        let (start, first, direct, direct_first) =
            if option_first { (1, 3, 5, 12) } else { (2, 7, 1, 3) };
        for attack in 0..10 {
            let mut h = claim.clone();
            let blocks = &mut h.graph.functions[0].blocks;
            match attack {
                0 | 1 => {
                    let graph::Terminator::ClosedEnumMatch { arms, .. } = &mut blocks[0].terminator
                    else {
                        panic!("match")
                    };
                    let index = if attack == 0 {
                        usize::from(!option_first)
                    } else {
                        usize::from(option_first)
                    };
                    let target = if attack == 0 { start } else { direct };
                    let ty = borrow(if attack == 0 { 2 } else { 3 });
                    arms[index].binding.as_mut().expect("binding").ty = ty;
                    blocks[target].parameters[0].ty = ty;
                }
                2 | 3 => {
                    let target = if attack == 2 { start + 3 } else { direct };
                    let position = usize::from(attack == 3);
                    let graph::Operation::ClosedEnumConstruct { ordinal, .. } =
                        &mut blocks[target].instructions[position].operation
                    else {
                        panic!("construct")
                    };
                    *ordinal = 1 - *ordinal;
                }
                4 | 5 => {
                    let (result, id) =
                        if attack == 4 { (first + 3, first) } else { (direct_first + 1, first) };
                    h.extensions[0]
                        .iter_mut()
                        .find(|e| e.result == result)
                        .expect("leaf")
                        .operation = raw::Operation::CloneBorrowedString(id);
                }
                6 => blocks[start + 2].instructions[0].result.ty = graph::Type::Stored(3),
                7 => {
                    let graph::Terminator::ClosedEnumMatch { arms, .. } = &mut blocks[0].terminator
                    else {
                        panic!("match")
                    };
                    arms[1].ordinal = 0;
                }
                8 => {
                    let graph::Terminator::ClosedEnumMatch { ty, .. } =
                        &mut blocks[start].terminator
                    else {
                        panic!("inner")
                    };
                    *ty = 4;
                }
                _ => {
                    let graph::Operation::ClosedEnumConstruct { payload, .. } =
                        &mut blocks[direct].instructions[1].operation
                    else {
                        panic!("direct")
                    };
                    *payload = Some(0);
                }
            }
            let Failure::Diagnostics(v) =
                typed(&h, &layouts, &s).expect_err("heterogeneous shape attack")
            else {
                panic!("diagnostic")
            };
            assert_eq!(v[0].code, "ZRYNA-I7001");
            typed(&claim, &layouts, &s).expect("pristine typing recovery");
        }
    }
}

#[test]
fn independent_asymmetric_owner_replay_rejects_exclusive_ended_foreign_and_unbalanced_loans() {
    for option_first in [true, false] {
        let (claim, layouts, _) = fixture(option_first);
        let (start, first, direct_first) = if option_first { (1, 3, 12) } else { (2, 7, 3) };
        for attack in 0..7 {
            let mut h = claim.clone();
            match attack {
                0 => {
                    h.extensions[0].retain(|e| e.result != 1 && e.result != 19);
                    h.graph.functions[0].blocks[0].instructions[0] =
                        instruction(1, graph::Type::Stored(1), graph::Operation::I32Literal(0));
                    h.extensions[0].iter_mut().find(|e| e.result == 2).expect("borrow").operation =
                        raw::Operation::Borrow { value: 0, exclusive: true };
                }
                1 => {
                    h.extensions[0]
                        .iter_mut()
                        .find(|e| e.result == first + 3)
                        .expect("leaf")
                        .operation = raw::Operation::EndLoan(first + 2);
                    h.graph.functions[0].blocks[start + 2].instructions[0].result.ty =
                        graph::Type::Unit;
                    h.extensions[0]
                        .iter_mut()
                        .find(|e| e.result == first + 5)
                        .expect("end")
                        .operation = raw::Operation::CloneBorrowedString(first + 2);
                    h.graph.functions[0].blocks[start + 2].instructions[2].result.ty =
                        graph::Type::Stored(2);
                }
                2 => {
                    h.extensions[0]
                        .iter_mut()
                        .find(|e| e.result == first + 5)
                        .expect("end")
                        .operation = raw::Operation::EndLoan(first);
                }
                3 => {
                    h.extensions[0]
                        .iter_mut()
                        .find(|e| e.result == direct_first + 1)
                        .expect("leaf")
                        .operation = raw::Operation::CloneBorrowedString(0);
                }
                4 => {
                    h.extensions[0]
                        .iter_mut()
                        .find(|e| e.result == direct_first + 3)
                        .expect("end")
                        .operation = raw::Operation::EndLoan(2);
                }
                5 => {
                    h.extensions[0]
                        .iter_mut()
                        .find(|e| e.result == first + 8)
                        .expect("end")
                        .operation = raw::Operation::EndLoan(1);
                }
                _ => {
                    h.extensions[0].iter_mut().find(|e| e.result == 18).expect("drop").operation =
                        raw::Operation::Drop(0);
                }
            }
            let Failure::Diagnostics(v) = derive(&h.graph, &h.extensions, &layouts)
                .expect_err("asymmetric owner/loan attack")
            else {
                panic!("diagnostic")
            };
            assert_eq!(v[0].code, "ZRYNA-I7001");
            derive(&claim.graph, &claim.extensions, &layouts).expect("pristine owner recovery");
        }
    }
}
