//! Independent typed joins and cleanup; this graph never enters a source producer.
use super::*;
use zryna_source::{SourceFileInput, SourceMap};

fn fixture() -> (raw::Program, VerifiedLayouts, SourceMap) {
    let (mut program, _, layouts) = tests::fixture();
    let blocks = &mut program.functions[0].blocks;
    let mut instruction = blocks[3].instructions[0].clone();
    instruction.result.id = 2;
    blocks[1].instructions = vec![instruction.clone()];
    instruction.result.id = 3;
    instruction.result.ty = graph::Type::Stored(2);
    instruction.operation = graph::Operation::Unit;
    blocks[1].instructions.push(instruction.clone());
    instruction.result.id = 4;
    instruction.result.ty = graph::Type::Unit;
    blocks[1].instructions.push(instruction.clone());
    blocks[1].terminator = graph::Terminator::Jump(graph::Edge { target: 3, arguments: vec![2] });
    instruction.result.id = 5;
    instruction.result.ty = graph::Type::Stored(1);
    instruction.operation = graph::Operation::I32Literal(9);
    blocks[2].instructions = vec![instruction.clone()];
    instruction.result.id = 6;
    instruction.result.ty = graph::Type::Stored(2);
    instruction.operation = graph::Operation::Unit;
    blocks[2].instructions.push(instruction.clone());
    instruction.result.id = 7;
    instruction.result.ty = graph::Type::Unit;
    blocks[2].instructions.push(instruction.clone());
    blocks[2].terminator = graph::Terminator::Jump(graph::Edge { target: 3, arguments: vec![5] });
    blocks[3].parameters = vec![graph::Definition { id: 8, ty: graph::Type::Stored(1) }];
    instruction.result.id = 9;
    blocks[3].instructions = vec![instruction];
    blocks[3].terminator = graph::Terminator::Return(8);
    let extensions = vec![vec![
        raw::Extension { result: 3, operation: raw::Operation::StringLiteral("左".into()) },
        raw::Extension { result: 4, operation: raw::Operation::Drop(3) },
        raw::Extension { result: 6, operation: raw::Operation::StringLiteral(Vec::new()) },
        raw::Extension { result: 7, operation: raw::Operation::Drop(6) },
        raw::Extension { result: 9, operation: raw::Operation::Drop(0) },
    ]];
    let sources =
        SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text: "x".into() }])
            .expect("source span authority");
    (raw::Program { graph: program, extensions, plans: vec![] }, layouts, sources)
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
        &crate::generic_v1::inventory::Inventory { generic_count: 0, declarations: vec![0] },
    )
}

#[test]
fn independent_scalar_phi_has_exact_edge_types_dominance_and_root_cleanup() {
    let (claim, layouts, sources) = fixture();
    typed(&claim, &layouts, &sources).expect("hand-authored scalar phi is typed");
    let plans = derive(&claim.graph, &claim.extensions, &layouts).expect("root survives both arms");
    assert_eq!(
        plans[0].steps,
        vec![
            raw::Step { block: 1, position: 1, failure: true, end_loans: vec![], cleanup: vec![0] },
            raw::Step { block: 2, position: 1, failure: true, end_loans: vec![], cleanup: vec![0] },
            raw::Step { block: 3, position: 1, failure: false, end_loans: vec![], cleanup: vec![] },
        ]
    );
    for mutation in 0..5 {
        let mut hostile = claim.clone();
        let graph::Terminator::Jump(edge) = &mut hostile.graph.functions[0].blocks[1].terminator
        else {
            panic!("jump");
        };
        match mutation {
            0 => edge.arguments.clear(),
            1 => edge.arguments.push(2),
            2 => edge.arguments[0] = 1, // bool cannot satisfy i32.
            3 => edge.arguments[0] = 5, // sibling arm does not dominate.
            _ => edge.arguments[0] = u32::MAX,
        }
        assert!(matches!(typed(&hostile, &layouts, &sources), Err(Failure::Diagnostics(_))));
        typed(&claim, &layouts, &sources).expect("pristine typed recovery");
    }
    for mutation in 0..3 {
        let mut hostile = claim.extensions.clone();
        match mutation {
            0 => {
                hostile[0].remove(1);
            } // Left local leaks into merge.
            1 => hostile[0][1].operation = raw::Operation::Drop(0),
            _ => hostile[0][3].operation = raw::Operation::Drop(0),
        }
        assert!(matches!(derive(&claim.graph, &hostile, &layouts), Err(Failure::Diagnostics(_))));
        assert_eq!(derive(&claim.graph, &claim.extensions, &layouts).expect("recovery"), plans);
    }
}
