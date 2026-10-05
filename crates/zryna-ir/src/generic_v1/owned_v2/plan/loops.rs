//! Independent raw loop claims: no source lowering can repair these states or cleanup steps.
use super::*;

fn edge(target: u32) -> graph::Edge {
    graph::Edge { target, arguments: vec![] }
}

fn loop_fixture() -> (graph::Program, Vec<Vec<raw::Extension>>, VerifiedLayouts) {
    let (mut program, _, layouts) = tests::fixture();
    let blocks = &mut program.functions[0].blocks;
    blocks[0].terminator = graph::Terminator::Jump(edge(1));
    blocks[1].terminator = graph::Terminator::Branch { condition: 1, yes: edge(2), no: edge(3) };
    blocks[2].instructions[0].result.ty = graph::Type::Stored(2);
    let mut drop = blocks[1].instructions[0].clone();
    drop.result.id = 4;
    blocks[2].instructions.push(drop);
    blocks[2].terminator = graph::Terminator::Jump(edge(1));
    blocks[3].instructions[0].result.id = 5;
    blocks[3].terminator = graph::Terminator::Return(5);
    let effects = vec![vec![
        raw::Extension { result: 3, operation: raw::Operation::StringLiteral(b"iteration".into()) },
        raw::Extension { result: 4, operation: raw::Operation::Drop(3) },
    ]];
    (program, effects, layouts)
}

fn rejects(
    program: &graph::Program,
    effects: &[Vec<raw::Extension>],
    layouts: &VerifiedLayouts,
    message: &str,
) {
    let Failure::Diagnostics(errors) = derive(program, effects, layouts).expect_err(message) else {
        panic!("independent IR diagnostic")
    };
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "ZRYNA-I7001");
    assert_eq!(errors[0].message, message);
}

#[test]
fn independent_loop_replay_keeps_exact_header_state_and_failure_cleanup() {
    let (program, effects, layouts) = loop_fixture();
    let plan = derive(&program, &effects, &layouts).expect("body local released each iteration");
    assert_eq!(
        plan[0].steps,
        vec![
            raw::Step { block: 2, position: 0, failure: true, end_loans: vec![], cleanup: vec![0] },
            raw::Step {
                block: 3,
                position: 1,
                failure: false,
                end_loans: vec![],
                cleanup: vec![0]
            },
        ]
    );
    for operation in [None, Some(raw::Operation::Drop(0))] {
        let mut hostile = effects.clone();
        if let Some(operation) = operation {
            hostile[0][1].operation = operation;
        } else {
            hostile[0].pop();
        }
        rejects(&program, &hostile, &layouts, "owner/loan state differs at loop backedge");
    }
    derive(&program, &effects, &layouts).expect("pristine recovery without header repair");
}

#[test]
fn independent_loop_replay_rejects_retained_body_loans_and_loan_edge_arguments() {
    let (mut program, _, layouts) = loop_fixture();
    program.functions[0].blocks[2].instructions[0].result.ty =
        graph::Type::Borrow { referent: 2, exclusive: false };
    let effects = vec![vec![
        raw::Extension {
            result: 3,
            operation: raw::Operation::Borrow { value: 0, exclusive: false },
        },
        raw::Extension { result: 4, operation: raw::Operation::EndLoan(3) },
    ]];
    derive(&program, &effects, &layouts).expect("iteration loan ends before backedge");
    let mut hostile = effects.clone();
    hostile[0].pop();
    rejects(&program, &hostile, &layouts, "owner/loan state differs at loop backedge");
    program.functions[0].blocks[1].parameters.push(graph::Definition {
        id: 2,
        ty: graph::Type::Borrow { referent: 2, exclusive: false },
    });
    program.functions[0].blocks[1].instructions[0].result.id = 3;
    program.functions[0].blocks[2].instructions[0].result.id = 4;
    program.functions[0].blocks[2].instructions[1].result.id = 5;
    program.functions[0].blocks[3].instructions[0].result.id = 6;
    program.functions[0].blocks[3].terminator = graph::Terminator::Return(6);
    hostile[0][0].result = 4;
    program.functions[0].blocks[2].terminator =
        graph::Terminator::Jump(graph::Edge { target: 1, arguments: vec![4] });
    rejects(&program, &hostile, &layouts, "loan-carrying ordinary CFG edge");
}

#[test]
fn independent_loop_replay_handles_multiple_latches_and_copy_header_arguments() {
    let (mut program, effects, layouts) = loop_fixture();
    let mut latch = program.functions[0].blocks[1].clone();
    latch.id = 3;
    latch.instructions[0].result.id = 5;
    latch.terminator = graph::Terminator::Jump(edge(1));
    program.functions[0].blocks[2].terminator =
        graph::Terminator::Branch { condition: 1, yes: edge(1), no: edge(3) };
    program.functions[0].blocks[3].id = 4;
    program.functions[0].blocks[3].instructions[0].result.id = 6;
    program.functions[0].blocks[3].terminator = graph::Terminator::Return(6);
    program.functions[0].blocks[1].terminator =
        graph::Terminator::Branch { condition: 1, yes: edge(2), no: edge(4) };
    program.functions[0].blocks.insert(3, latch);
    derive(&program, &effects, &layouts).expect("both latches restore the same header");

    let (mut program, mut effects, layouts) = loop_fixture();
    let blocks = &mut program.functions[0].blocks;
    blocks[0].terminator = graph::Terminator::Jump(graph::Edge { target: 1, arguments: vec![1] });
    blocks[1].parameters.push(graph::Definition { id: 2, ty: graph::Type::Stored(0) });
    blocks[1].instructions[0].result.id = 3;
    blocks[1].terminator = graph::Terminator::Branch { condition: 2, yes: edge(2), no: edge(3) };
    blocks[2].instructions[0].result.id = 4;
    blocks[2].instructions[1].result.id = 5;
    let mut flag = blocks[0].instructions[0].clone();
    flag.result.id = 6;
    flag.operation = graph::Operation::BoolLiteral(false);
    blocks[2].instructions.push(flag);
    blocks[2].terminator = graph::Terminator::Jump(graph::Edge { target: 1, arguments: vec![6] });
    blocks[3].instructions[0].result.id = 7;
    blocks[3].terminator = graph::Terminator::Return(7);
    effects[0][0].result = 4;
    effects[0][1].result = 5;
    effects[0][1].operation = raw::Operation::Drop(4);
    derive(&program, &effects, &layouts).expect("Copy header arguments preserve owner state");
}

#[test]
fn independent_loop_replay_rejects_irreducible_unreachable_and_foreign_edges() {
    let (program, effects, layouts) = loop_fixture();
    let mut hostile = program.clone();
    hostile.functions[0].blocks[0].terminator =
        graph::Terminator::Branch { condition: 1, yes: edge(1), no: edge(2) };
    hostile.functions[0].blocks[1].terminator = graph::Terminator::Jump(edge(2));
    hostile.functions[0].blocks[2].terminator =
        graph::Terminator::Branch { condition: 1, yes: edge(1), no: edge(3) };
    rejects(
        &hostile,
        &effects,
        &layouts,
        "owned graph is irreducible or has unavailable forward state",
    );
    hostile = program.clone();
    hostile.functions[0].blocks[1].terminator = graph::Terminator::Jump(edge(2));
    rejects(
        &hostile,
        &effects,
        &layouts,
        "owned CFG has unreachable claims or no complete dominators",
    );
    for target in [0, 99] {
        hostile = program.clone();
        hostile.functions[0].blocks[2].terminator = graph::Terminator::Jump(edge(target));
        rejects(&hostile, &effects, &layouts, "owned edge targets entry or an unknown block");
    }
    derive(&program, &effects, &layouts).expect("pristine recovery after topology forgeries");
}

#[test]
fn independent_nested_loop_replay_retains_each_distinct_header_before_backedges() {
    let (mut program, mut effects, layouts) = loop_fixture();
    let mut inner = program.functions[0].blocks[1].clone();
    inner.id = 2;
    inner.instructions[0].result.id = 3;
    inner.terminator = graph::Terminator::Branch { condition: 1, yes: edge(3), no: edge(4) };
    let mut outer_latch = program.functions[0].blocks[1].clone();
    outer_latch.id = 4;
    outer_latch.instructions[0].result.id = 6;
    outer_latch.terminator = graph::Terminator::Jump(edge(1));
    let blocks = &mut program.functions[0].blocks;
    blocks[1].terminator = graph::Terminator::Branch { condition: 1, yes: edge(2), no: edge(5) };
    blocks[2].id = 3;
    blocks[2].instructions[0].result.id = 4;
    blocks[2].instructions[1].result.id = 5;
    blocks[2].terminator = graph::Terminator::Jump(edge(2));
    blocks[3].id = 5;
    blocks[3].instructions[0].result.id = 7;
    blocks[3].terminator = graph::Terminator::Return(7);
    blocks.insert(2, inner);
    blocks.insert(4, outer_latch);
    effects[0][0].result = 4;
    effects[0][1].result = 5;
    effects[0][1].operation = raw::Operation::Drop(4);
    let plan = derive(&program, &effects, &layouts).expect("separate nested header snapshots");
    assert_eq!(plan[0].steps[0].block, 3);
    assert_eq!(plan[0].steps[0].cleanup, vec![0]);
    let mut hostile = effects.clone();
    hostile[0].push(raw::Extension { result: 6, operation: raw::Operation::Drop(0) });
    rejects(&program, &hostile, &layouts, "owner/loan state differs at loop backedge");
    derive(&program, &effects, &layouts).expect("nested pristine recovery");
}

#[test]
fn independent_loop_resource_boundary_charges_retained_headers_and_every_failure_prefix() {
    let make = |n: u32| {
        let (mut program, _, layouts) = loop_fixture();
        let f = &mut program.functions[0];
        f.parameters = vec![graph::Type::Stored(2); 200];
        f.parameters.push(graph::Type::Stored(0));
        f.blocks[0].parameters = f
            .parameters
            .iter()
            .enumerate()
            .map(|(id, &ty)| graph::Definition { id: u32::try_from(id).expect("id"), ty })
            .collect();
        f.blocks[0].instructions.clear();
        f.blocks[1].instructions[0].result.id = 201;
        f.blocks[1].terminator =
            graph::Terminator::Branch { condition: 200, yes: edge(2), no: edge(3) };
        let mut instructions = Vec::new();
        let mut effects = Vec::new();
        for slot in 0..n {
            let id = 202 + slot * 2;
            for (offset, ty) in [(0, graph::Type::Stored(2)), (1, graph::Type::Unit)] {
                let mut ins = f.blocks[1].instructions[0].clone();
                ins.result = graph::Definition { id: id + offset, ty };
                instructions.push(ins);
            }
            effects.push(raw::Extension {
                result: id,
                operation: raw::Operation::StringLiteral(b"iteration".into()),
            });
            effects.push(raw::Extension { result: id + 1, operation: raw::Operation::Drop(id) });
        }
        f.blocks[2].instructions = instructions;
        f.blocks[3].instructions[0].result.id = 202 + n * 2;
        f.blocks[3].terminator = graph::Terminator::Return(202 + n * 2);
        (program, vec![effects], layouts)
    };
    // Four blocks (32), four stored edges (8), one header (401), four edge
    // snapshots (1604), return (201), and 201 units per fallible String creation.
    let exact = (MAX_OWNER_UNITS - 2246) / 201;
    assert_eq!(exact, 5205);
    for (n, accepted) in [(exact, true), (exact + 1, false), (1, true)] {
        let (program, effects, layouts) = make(u32::try_from(n).expect("bounded count"));
        match derive(&program, &effects, &layouts) {
            Ok(plan) => {
                assert!(accepted);
                assert_eq!(plan[0].steps.len(), n + 1);
                assert_eq!(plan[0].steps[0].cleanup, (0..200).rev().collect::<Vec<_>>());
            }
            Err(Failure::Diagnostics(errors)) => {
                assert!(!accepted);
                assert_eq!(errors.len(), 1);
                assert_eq!(errors[0].code, "ZRYNA-I3201");
            }
            Err(other) => panic!("unexpected failure {other:?}"),
        }
    }
}
