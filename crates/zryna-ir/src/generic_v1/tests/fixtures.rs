//! Complete synthetic IR graphs, using genuine immutable source/layout carriers.

use super::super::{Failure, raw, validate_closed_graph};
use zryna_layout::{
    StorageTarget,
    generic_v1::{self as layout, raw as types},
};
use zryna_source::{SourceFileInput, SourceMap, UntrustedSpan};

pub(super) struct Fixture {
    pub sources: SourceMap,
    pub linear: layout::VerifiedLayouts,
    pub linux: layout::VerifiedLayouts,
    pub program: raw::Program,
}

pub(super) fn key(tag: u8, module: u32, function: u32, args: &[&[u8]]) -> Vec<u8> {
    let mut key = vec![tag];
    key.extend_from_slice(&module.to_le_bytes());
    key.extend_from_slice(&function.to_le_bytes());
    if tag == 0x40 {
        key.extend_from_slice(&u32::try_from(args.len()).expect("count").to_le_bytes());
    }
    for arg in args {
        key.extend_from_slice(&u32::try_from(arg.len()).expect("length").to_le_bytes());
        key.extend_from_slice(arg);
    }
    key
}

pub(super) fn definition(id: u32, ty: raw::Type) -> raw::Definition {
    raw::Definition { id, ty }
}

impl Fixture {
    pub fn new(result: bool) -> Self {
        let sources = SourceMap::build(vec![SourceFileInput {
            path: "main.zry".into(),
            text: "//π\r\nfunction f(): i32 { return 7; }".into(),
        }])
        .expect("source carrier");
        let file = sources.verify_file_id(0).expect("file");
        let mut nodes = [
            zryna_layout::raw::TypeKind::Bool,
            zryna_layout::raw::TypeKind::I32,
            zryna_layout::raw::TypeKind::String,
        ]
        .into_iter()
        .enumerate()
        .map(|(id, kind)| types::TypeNode {
            id: types::NodeId(u32::try_from(id).expect("id")),
            span: None,
            kind: types::TypeKind::Base(kind),
        })
        .collect::<Vec<_>>();
        let family = if result {
            types::TypeKind::Result { okay: types::NodeId(1), error: types::NodeId(0) }
        } else {
            types::TypeKind::Option { argument: types::NodeId(1) }
        };
        nodes.push(types::TypeNode { id: types::NodeId(3), span: None, kind: family });
        let graph = types::Graph {
            modules: vec![types::Module {
                id: types::ModuleId(0),
                source_file: file,
                data_declarations: 0,
            }],
            declarations: vec![],
            types: nodes,
            program_roots: vec![types::NodeId(3)],
        };
        Self::from_graph(sources, &graph)
    }

    pub fn from_graph(sources: SourceMap, graph: &types::Graph) -> Self {
        let linear =
            layout::verify(graph, &sources, StorageTarget::Linear32V1).expect("linear carrier");
        let linux =
            layout::verify(graph, &sources, StorageTarget::LinuxX8664V1).expect("linux carrier");
        let span = UntrustedSpan { file: 0, start: 0, end: 36 };
        let program = raw::Program {
            modules: vec![raw::Module { id: 0, functions: 1 }],
            declarations: vec![raw::Declaration { module: 0, function: 0, parameters: 0, span }],
            type_keys: linear.types().map(|ty| ty.key().to_vec()).collect(),
            universe: *linear.universe_identity(),
            linear32: *linear.fingerprint(),
            linux_x86_64: *linux.fingerprint(),
            functions: vec![raw::Function {
                key: key(0x41, 0, 0, &[]),
                span,
                public_export: None,
                parameters: vec![],
                result: raw::Type::Stored(1),
                blocks: vec![raw::Block {
                    id: 0,
                    parameters: vec![],
                    instructions: vec![raw::Instruction {
                        result: definition(0, raw::Type::Stored(1)),
                        span,
                        operation: raw::Operation::I32Literal(7),
                    }],
                    span,
                    terminator: raw::Terminator::Return(0),
                }],
            }],
        };
        Self { sources, linear, linux, program }
    }

    pub fn check(&self) -> Result<(), Failure> {
        validate_closed_graph(&self.program, &self.sources, &self.linear, &self.linux)
    }

    pub fn enum_match() -> Self {
        let mut fixture = Self::new(false);
        let span = fixture.program.functions[0].span;
        fixture.program.functions[0].blocks = vec![
            raw::Block {
                id: 0,
                parameters: vec![],
                instructions: vec![
                    raw::Instruction {
                        result: definition(0, raw::Type::Stored(1)),
                        span,
                        operation: raw::Operation::I32Literal(7),
                    },
                    raw::Instruction {
                        result: definition(1, raw::Type::Stored(3)),
                        span,
                        operation: raw::Operation::ClosedEnumConstruct {
                            ty: 3,
                            ordinal: 1,
                            payload: Some(0),
                        },
                    },
                ],
                span,
                terminator: raw::Terminator::ClosedEnumMatch {
                    ty: 3,
                    scrutinee: 1,
                    mode: raw::MatchMode::Value,
                    arms: vec![
                        raw::Arm {
                            ordinal: 0,
                            binding: None,
                            edge: raw::Edge { target: 1, arguments: vec![] },
                        },
                        raw::Arm {
                            ordinal: 1,
                            binding: Some(definition(3, raw::Type::Stored(1))),
                            edge: raw::Edge { target: 2, arguments: vec![] },
                        },
                    ],
                },
            },
            raw::Block {
                id: 1,
                parameters: vec![],
                instructions: vec![raw::Instruction {
                    result: definition(2, raw::Type::Stored(1)),
                    span,
                    operation: raw::Operation::I32Literal(0),
                }],
                span,
                terminator: raw::Terminator::Return(2),
            },
            raw::Block {
                id: 2,
                parameters: vec![definition(3, raw::Type::Stored(1))],
                instructions: vec![],
                span,
                terminator: raw::Terminator::Return(3),
            },
        ];
        fixture
    }

    pub fn generic_calls() -> Self {
        let mut fixture = Self::new(false);
        let span = fixture.program.functions[0].span;
        fixture.program.modules[0].functions = 2;
        fixture.program.declarations = vec![
            raw::Declaration { module: 0, function: 0, parameters: 1, span },
            raw::Declaration { module: 0, function: 1, parameters: 0, span },
        ];
        let instances = [0u8, 1]
            .into_iter()
            .map(|primitive| raw::Function {
                key: key(0x40, 0, 0, &[&[primitive]]),
                span,
                public_export: None,
                parameters: vec![raw::Type::Stored(u32::from(primitive))],
                result: raw::Type::Stored(u32::from(primitive)),
                blocks: vec![raw::Block {
                    id: 0,
                    parameters: vec![definition(0, raw::Type::Stored(u32::from(primitive)))],
                    instructions: vec![],
                    span,
                    terminator: raw::Terminator::Return(0),
                }],
            })
            .collect::<Vec<_>>();
        let mut root = fixture.program.functions.remove(0);
        root.key = key(0x41, 0, 1, &[]);
        root.blocks[0].instructions.extend([
            raw::Instruction {
                result: definition(1, raw::Type::Stored(1)),
                span,
                operation: raw::Operation::ClosedGenericCall { instance: 1, arguments: vec![0] },
            },
            raw::Instruction {
                result: definition(2, raw::Type::Stored(0)),
                span,
                operation: raw::Operation::BoolLiteral(true),
            },
            raw::Instruction {
                result: definition(3, raw::Type::Stored(0)),
                span,
                operation: raw::Operation::ClosedGenericCall { instance: 0, arguments: vec![2] },
            },
        ]);
        root.blocks[0].terminator = raw::Terminator::Return(1);
        fixture.program.functions = instances;
        fixture.program.functions.push(root);
        fixture
    }
}

pub(super) fn rejection(result: Result<(), Failure>, expected: &str) {
    let Failure::Diagnostics(errors) = result.expect_err("raw claim must fail") else {
        panic!("stable diagnostic")
    };
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), expected);
}
