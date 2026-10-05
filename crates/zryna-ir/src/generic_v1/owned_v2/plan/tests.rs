//! Hand-authored path states do not pass through the source producer or its replay.
use super::*;
use zryna_layout::{StorageTarget, generic_v1 as layout};
use zryna_source::{SourceFileInput, SourceMap, UntrustedSpan};

pub(super) fn fixture() -> (graph::Program, Vec<Vec<raw::Extension>>, VerifiedLayouts) {
    let sources =
        SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text: "x".into() }])
            .expect("source");
    let types = [
        zryna_layout::raw::TypeKind::Bool,
        zryna_layout::raw::TypeKind::I32,
        zryna_layout::raw::TypeKind::String,
    ]
    .into_iter()
    .enumerate()
    .map(|(id, kind)| layout::raw::TypeNode {
        id: layout::raw::NodeId(u32::try_from(id).expect("id")),
        span: None,
        kind: layout::raw::TypeKind::Base(kind),
    })
    .collect();
    let layouts = layout::verify(
        &layout::raw::Graph {
            modules: vec![layout::raw::Module {
                id: layout::raw::ModuleId(0),
                source_file: sources.verify_file_id(0).expect("file"),
                data_declarations: 0,
            }],
            declarations: vec![],
            types,
            program_roots: vec![
                layout::raw::NodeId(0),
                layout::raw::NodeId(1),
                layout::raw::NodeId(2),
            ],
        },
        &sources,
        StorageTarget::Linear32V1,
    )
    .expect("layouts");
    let span = UntrustedSpan { file: 0, start: 0, end: 1 };
    let definition = |id, ty| graph::Definition { id, ty };
    let instruction =
        |id, ty, operation| graph::Instruction { result: definition(id, ty), span, operation };
    let edge = |target| graph::Edge { target, arguments: vec![] };
    let blocks = vec![
        graph::Block {
            id: 0,
            parameters: vec![definition(0, graph::Type::Stored(2))],
            instructions: vec![instruction(
                1,
                graph::Type::Stored(0),
                graph::Operation::BoolLiteral(true),
            )],
            span,
            terminator: graph::Terminator::Branch { condition: 1, yes: edge(1), no: edge(2) },
        },
        graph::Block {
            id: 1,
            parameters: vec![],
            instructions: vec![instruction(2, graph::Type::Unit, graph::Operation::Unit)],
            span,
            terminator: graph::Terminator::Jump(edge(3)),
        },
        graph::Block {
            id: 2,
            parameters: vec![],
            instructions: vec![instruction(3, graph::Type::Unit, graph::Operation::Unit)],
            span,
            terminator: graph::Terminator::Jump(edge(3)),
        },
        graph::Block {
            id: 3,
            parameters: vec![],
            instructions: vec![instruction(
                4,
                graph::Type::Stored(1),
                graph::Operation::I32Literal(7),
            )],
            span,
            terminator: graph::Terminator::Return(4),
        },
    ];
    let program = graph::Program {
        modules: vec![],
        declarations: vec![],
        type_keys: layouts.types().map(|ty| ty.key().to_vec()).collect(),
        universe: *layouts.universe_identity(),
        linear32: *layouts.fingerprint(),
        linux_x86_64: [0; 32],
        functions: vec![graph::Function {
            key: vec![0x41],
            span,
            public_export: None,
            parameters: vec![graph::Type::Stored(2)],
            result: graph::Type::Stored(1),
            blocks,
        }],
    };
    let extensions = vec![vec![
        raw::Extension { result: 2, operation: raw::Operation::Drop(0) },
        raw::Extension { result: 3, operation: raw::Operation::Drop(0) },
    ]];
    (program, extensions, layouts)
}

#[test]
fn independent_branch_replay_rejects_unequal_owner_states_in_both_target_orders() {
    let (mut program, extensions, layouts) = fixture();
    for reversed in [false, true] {
        if reversed {
            let graph::Terminator::Branch { yes, no, .. } =
                &mut program.functions[0].blocks[0].terminator
            else {
                panic!("branch")
            };
            std::mem::swap(yes, no);
        }
        let plans =
            derive(&program, &extensions, &layouts).expect("both paths consume exact owner");
        assert_eq!(plans[0].steps[0].cleanup, Vec::<u32>::new());
        for omitted in [0, 1] {
            let mut hostile = extensions.clone();
            hostile[0].remove(omitted);
            let Failure::Diagnostics(errors) = derive(&program, &hostile, &layouts)
                .expect_err("no producer can repair unequal raw states")
            else {
                panic!("IR diagnostic")
            };
            assert_eq!(errors[0].code, "ZRYNA-I7001");
            assert_eq!(errors[0].message, "owner/loan state differs at CFG merge");
        }
        derive(&program, &extensions, &layouts).expect("pristine independent recovery");
    }
}

#[test]
fn independent_branch_replay_rejects_unequal_loans_before_join_cleanup() {
    let (mut program, _, layouts) = fixture();
    program.functions[0].blocks[1].instructions[0].result.ty =
        graph::Type::Borrow { referent: 2, exclusive: false };
    let extensions = vec![vec![raw::Extension {
        result: 2,
        operation: raw::Operation::Borrow { value: 0, exclusive: false },
    }]];
    let Failure::Diagnostics(errors) = derive(&program, &extensions, &layouts)
        .expect_err("a future cleanup claim cannot repair an unequal incoming loan")
    else {
        panic!("IR diagnostic")
    };
    assert_eq!(errors[0].code, "ZRYNA-I7001");
    assert_eq!(errors[0].message, "owner/loan state differs at CFG merge");
}
