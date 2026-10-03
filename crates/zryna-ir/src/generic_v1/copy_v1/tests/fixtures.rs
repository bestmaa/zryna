//! Complete original syntax plus independently authored family layout claims.

use crate::generic_v1::{copy_v1::produce_claim, raw};
use zryna_layout::{
    StorageTarget,
    generic_v1::{VerifiedLayouts, raw as types, verify},
};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::v5::{VerifiedProjectSyntaxV5, decode_snapshot, verify_snapshot};

const SOURCE: &str = include_str!("../../../../../../tests/m7-generic-copy-fixtures/main.zry");
const SNAPSHOT: &[u8] =
    include_bytes!("../../../../../../tests/m7-generic-copy-fixtures/reference.json");

pub(super) struct Fixture {
    pub sources: SourceMap,
    pub syntax: VerifiedProjectSyntaxV5,
    pub linear: VerifiedLayouts,
    pub linux: VerifiedLayouts,
    pub raw: raw::Program,
}

pub(super) fn authorities(
    source: &str,
    snapshot: &[u8],
    families: bool,
) -> (SourceMap, VerifiedProjectSyntaxV5, VerifiedLayouts, VerifiedLayouts) {
    let sources =
        SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text: source.into() }])
            .expect("genuine fixture invariant");
    let syntax =
        verify_snapshot(decode_snapshot(snapshot).expect("genuine fixture invariant"), &sources)
            .expect("genuine fixture invariant");
    let mut nodes = [
        zryna_layout::raw::TypeKind::Bool,
        zryna_layout::raw::TypeKind::I32,
        zryna_layout::raw::TypeKind::String,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, kind)| types::TypeNode {
        id: types::NodeId(u32::try_from(index).expect("genuine fixture invariant")),
        span: None,
        kind: types::TypeKind::Base(kind),
    })
    .collect::<Vec<_>>();
    if families {
        nodes.extend([
            types::TypeNode {
                id: types::NodeId(3),
                span: None,
                kind: types::TypeKind::Option { argument: types::NodeId(1) },
            },
            types::TypeNode {
                id: types::NodeId(4),
                span: None,
                kind: types::TypeKind::Result { okay: types::NodeId(0), error: types::NodeId(1) },
            },
        ]);
    }
    let roots = nodes.iter().map(|node| node.id).collect();
    let graph = types::Graph {
        modules: vec![types::Module {
            id: types::ModuleId(0),
            source_file: sources.verify_file_id(0).expect("genuine fixture invariant"),
            data_declarations: 0,
        }],
        declarations: Vec::new(),
        types: nodes,
        program_roots: roots,
    };
    let linear =
        verify(&graph, &sources, StorageTarget::Linear32V1).expect("genuine fixture invariant");
    let linux =
        verify(&graph, &sources, StorageTarget::LinuxX8664V1).expect("genuine fixture invariant");
    (sources, syntax, linear, linux)
}

pub(super) fn key(function: u32, argument: u8) -> Vec<u8> {
    let mut key = vec![0x40];
    for lane in [0, function, 1, 1] {
        key.extend_from_slice(&lane.to_le_bytes());
    }
    key.push(argument);
    key
}

impl Fixture {
    pub fn new() -> Self {
        let (sources, syntax, linear, linux) = authorities(SOURCE, SNAPSHOT, true);
        let mut option = vec![0x14];
        option.extend_from_slice(&1u32.to_le_bytes());
        option.extend_from_slice(&1u32.to_le_bytes());
        option.push(1);
        let mut identity_option = vec![0x40];
        for lane in [0u32, 0, 1, 10] {
            identity_option.extend_from_slice(&lane.to_le_bytes());
        }
        identity_option.extend_from_slice(&option);
        let keys = [key(0, 0), key(0, 1), identity_option, key(1, 1), key(6, 1)];
        let raw = produce_claim(
            &syntax,
            &sources,
            &linear,
            &linux,
            &keys.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        )
        .expect("genuine fixture invariant");
        Self { sources, syntax, linear, linux, raw }
    }
}
