//! Independent hand-authored CFGs, independent of owned source production.
//! These tests issue no `VerifiedOwnedProgram` and make no target-execution claim.
use super::*;
use zryna_layout::{StorageTarget, generic_v1 as layout};
use zryna_source::{SourceFileInput, SourceMap, UntrustedSpan};

pub(super) const OPTION: u32 = 3;
pub(super) const RESULT: u32 = 4;
const OPTION_KEY: &[u8] = &[0x14, 1, 0, 0, 0, 1, 0, 0, 0, 1];
const RESULT_KEY: &[u8] = &[0x15, 2, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0];
const AT: UntrustedSpan = UntrustedSpan { file: 0, start: 0, end: 1 };

pub(super) fn d(id: u32, ty: graph::Type) -> graph::Definition {
    graph::Definition { id, ty }
}
pub(super) fn stored(id: u32, ty: u32) -> graph::Definition {
    d(id, graph::Type::Stored(ty))
}
pub(super) fn ins(id: u32, ty: graph::Type, operation: graph::Operation) -> graph::Instruction {
    graph::Instruction { result: d(id, ty), span: AT, operation }
}
pub(super) fn int(id: u32, value: i32) -> graph::Instruction {
    ins(id, graph::Type::Stored(1), graph::Operation::I32Literal(value))
}
pub(super) fn boolean(id: u32, value: bool) -> graph::Instruction {
    ins(id, graph::Type::Stored(0), graph::Operation::BoolLiteral(value))
}
pub(super) fn construct(
    id: u32,
    ty: u32,
    ordinal: u32,
    payload: Option<u32>,
) -> graph::Instruction {
    ins(id, graph::Type::Stored(ty), graph::Operation::ClosedEnumConstruct { ty, ordinal, payload })
}
pub(super) fn placeholder(id: u32, ty: graph::Type) -> graph::Instruction {
    ins(id, ty, graph::Operation::Unit)
}
pub(super) fn e(target: u32, arguments: &[u32]) -> graph::Edge {
    graph::Edge { target, arguments: arguments.to_vec() }
}
pub(super) fn jump(target: u32, arguments: &[u32]) -> graph::Terminator {
    graph::Terminator::Jump(e(target, arguments))
}
pub(super) fn block(
    id: u32,
    parameters: Vec<graph::Definition>,
    instructions: Vec<graph::Instruction>,
    terminator: graph::Terminator,
) -> graph::Block {
    graph::Block { id, parameters, instructions, span: AT, terminator }
}
pub(super) fn arm(
    ordinal: u32,
    binding: Option<graph::Definition>,
    target: u32,
    arguments: &[u32],
) -> graph::Arm {
    graph::Arm { ordinal, binding, edge: e(target, arguments) }
}
pub(super) fn selection(ty: u32, scrutinee: u32, arms: Vec<graph::Arm>) -> graph::Terminator {
    graph::Terminator::ClosedEnumMatch { ty, scrutinee, mode: graph::MatchMode::Value, arms }
}

pub(super) fn authorities() -> (VerifiedLayouts, VerifiedLayouts, SourceMap) {
    let sources = SourceMap::build(vec![SourceFileInput {
        path: "independent.zry".into(),
        text: "x".into(),
    }])
    .expect("span authority only, no syntax/source producer");
    // Deliberately use noncanonical discovery order for the two families.
    // Node3 Result, Node4 Option; canonical branded IDs must still be Option3/Result4.
    let kinds = vec![
        layout::raw::TypeKind::Base(zryna_layout::raw::TypeKind::Bool),
        layout::raw::TypeKind::Base(zryna_layout::raw::TypeKind::I32),
        layout::raw::TypeKind::Base(zryna_layout::raw::TypeKind::String),
        layout::raw::TypeKind::Result {
            okay: layout::raw::NodeId(1),
            error: layout::raw::NodeId(0),
        },
        layout::raw::TypeKind::Option { argument: layout::raw::NodeId(1) },
    ];
    let graph = layout::raw::Graph {
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
                id: layout::raw::NodeId(u32::try_from(id).expect("small node")),
                span: None,
                kind,
            })
            .collect(),
        program_roots: (0..5).map(layout::raw::NodeId).collect(),
    };
    let linear = layout::verify(&graph, &sources, StorageTarget::Linear32V1)
        .expect("independent linear authority");
    let linux = layout::verify(&graph, &sources, StorageTarget::LinuxX8664V1)
        .expect("independent Linux authority");
    assert_eq!(linear.universe_identity(), linux.universe_identity());
    for layouts in [&linear, &linux] {
        let views = layouts.types().collect::<Vec<_>>();
        let expected = vec![vec![0], vec![1], vec![2], OPTION_KEY.to_vec(), RESULT_KEY.to_vec()];
        assert_eq!(views.iter().map(|t| t.key().to_vec()).collect::<Vec<_>>(), expected);
        assert_eq!(views[3].id().index(), OPTION);
        assert_eq!(views[4].id().index(), RESULT);
        assert_eq!(views[3].drop_kind(), 0);
        assert_eq!(views[4].drop_kind(), 0);
        assert_ne!(views[2].drop_kind(), 0);
        assert_eq!(
            views[3]
                .variants()
                .map(|(n, p)| (n, p.map(zryna_layout::generic_v1::TypeId::index)))
                .collect::<Vec<_>>(),
            [(0, None), (1, Some(1))]
        );
        assert_eq!(
            views[4]
                .variants()
                .map(|(n, p)| (n, p.map(zryna_layout::generic_v1::TypeId::index)))
                .collect::<Vec<_>>(),
            [(0, Some(1)), (1, Some(0))]
        );
        assert_eq!(
            views[3].arguments().map(zryna_layout::generic_v1::TypeId::index).collect::<Vec<_>>(),
            [1]
        );
        assert_eq!(
            views[4].arguments().map(zryna_layout::generic_v1::TypeId::index).collect::<Vec<_>>(),
            [1, 0]
        );
    }
    (linear, linux, sources)
}

pub(super) fn claim(
    blocks: Vec<graph::Block>,
    extensions: Vec<raw::Extension>,
    linear: &VerifiedLayouts,
    linux: &VerifiedLayouts,
) -> raw::Program {
    raw::Program {
        graph: graph::Program {
            modules: vec![graph::Module { id: 0, functions: 1 }],
            declarations: vec![graph::Declaration {
                module: 0,
                function: 0,
                parameters: 0,
                span: AT,
            }],
            type_keys: linear.types().map(|t| t.key().to_vec()).collect(),
            universe: *linear.universe_identity(),
            linear32: *linear.fingerprint(),
            linux_x86_64: *linux.fingerprint(),
            functions: vec![graph::Function {
                key: vec![0x41, 0, 0, 0, 0, 0, 0, 0, 0],
                span: AT,
                public_export: None,
                parameters: vec![graph::Type::Stored(2), graph::Type::Stored(0)],
                result: graph::Type::Stored(1),
                blocks,
            }],
        },
        extensions: vec![extensions],
        plans: vec![],
    }
}

pub(super) fn typed(
    claim: &raw::Program,
    linear: &VerifiedLayouts,
    linux: &VerifiedLayouts,
    sources: &SourceMap,
) -> Result<(), Failure> {
    let inventory = crate::generic_v1::inventory::check(&claim.graph, sources, linear, linux)?;
    for layouts in [linear, linux] {
        super::super::super::typed::check(claim, sources, layouts, &inventory)?;
    }
    Ok(())
}
