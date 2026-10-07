//! Synthetic complete raw graphs, not source-admissible program claims.

use super::super::raw;
use super::fixtures::{Fixture, definition, key, rejection};

#[test]
fn original_static_call_depth_exact_first_extra_and_pristine_replay() {
    fn chain(count: u32) -> Fixture {
        let mut f = Fixture::new(false);
        let span = f.program.functions[0].span;
        f.program.modules[0].functions = count;
        f.program.declarations = (0..count)
            .map(|function| raw::Declaration { module: 0, function, parameters: 0, span })
            .collect();
        f.program.functions = (0..count)
            .map(|function| raw::Function {
                key: key(0x41, 0, function, &[]),
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
                        operation: if function + 1 == count {
                            raw::Operation::I32Literal(0)
                        } else {
                            raw::Operation::SourceCall {
                                module: 0,
                                function: function + 1,
                                arguments: vec![],
                            }
                        },
                    }],
                    span,
                    terminator: raw::Terminator::Return(0),
                }],
            })
            .collect();
        f
    }
    chain(128).check().expect("exact static depth");
    rejection(chain(129).check(), "ZRYNA-I3201");
    chain(128).check().expect("pristine replay");
}

#[test]
fn inherited_value_exact_first_extra_and_recovery_are_checked_before_body_traversal() {
    fn values(count: u32) -> Fixture {
        let mut f = Fixture::new(false);
        let span = f.program.functions[0].span;
        f.program.functions[0].blocks[0].instructions = (0..count)
            .map(|id| raw::Instruction {
                result: definition(id, raw::Type::Stored(1)),
                span,
                operation: if id == 0 {
                    raw::Operation::I32Literal(0)
                } else {
                    raw::Operation::Copy { value: 0 }
                },
            })
            .collect();
        f.program.functions[0].blocks[0].terminator = raw::Terminator::Return(count - 1);
        f
    }
    values(16384).check().expect("exact inherited value arena");
    rejection(values(16385).check(), "ZRYNA-I3201");
    values(16384).check().expect("pristine replay");
}

#[test]
fn complete_generic_function_inventory_exact_first_extra_and_duplicate_key_recovery() {
    use zryna_layout::{generic_v1::raw as types, raw::TypeKind as Base};
    use zryna_source::{SourceFileInput, SourceMap};
    fn instances(count: u32) -> Fixture {
        let sources = SourceMap::build(vec![SourceFileInput {
            path: "main.zry".into(),
            text: "//π\r\nfunction f(): i32 { return 7; }".into(),
        }])
        .expect("source carrier");
        let file = sources.verify_file_id(0).expect("file");
        let mut nodes = [Base::Bool, Base::I32, Base::String]
            .into_iter()
            .enumerate()
            .map(|(id, kind)| types::TypeNode {
                id: types::NodeId(u32::try_from(id).expect("index")),
                span: None,
                kind: types::TypeKind::Base(kind),
            })
            .collect::<Vec<_>>();
        for length in 1..=count {
            let id = types::NodeId(u32::try_from(nodes.len()).expect("index"));
            nodes.push(types::TypeNode {
                id,
                span: None,
                kind: types::TypeKind::Base(Base::FixedArray {
                    element: types::NodeId(1),
                    length: u64::from(length),
                }),
            });
        }
        let graph = types::Graph {
            modules: vec![types::Module {
                id: types::ModuleId(0),
                source_file: file,
                data_declarations: 0,
            }],
            declarations: vec![],
            program_roots: nodes.iter().map(|node| node.id).collect(),
            types: nodes,
        };
        let mut f = Fixture::from_graph(sources, &graph);
        let span = f.program.functions[0].span;
        f.program.declarations[0].parameters = 1;
        f.program.functions = f
            .linear
            .types()
            .skip(3)
            .map(|ty| raw::Function {
                key: key(0x40, 0, 0, &[ty.key()]),
                span,
                public_export: None,
                parameters: vec![raw::Type::Stored(ty.id().index())],
                result: raw::Type::Stored(ty.id().index()),
                blocks: vec![raw::Block {
                    id: 0,
                    parameters: vec![definition(0, raw::Type::Stored(ty.id().index()))],
                    instructions: vec![],
                    span,
                    terminator: raw::Terminator::Return(0),
                }],
            })
            .collect();
        f
    }
    let exact = instances(4096);
    exact.check().expect("exact full function inventory");
    rejection(instances(4097).check(), "ZRYNA-I3201");
    let mut duplicate = instances(4096);
    duplicate.program.functions[1].key = duplicate.program.functions[0].key.clone();
    rejection(duplicate.check(), "ZRYNA-I7001");
    exact.check().expect("pristine replay");
}

#[test]
fn complete_nested_cfg_loops_exact_first_extra_and_recovery() {
    fn nested(depth: u32) -> Fixture {
        let mut f = Fixture::new(false);
        let span = f.program.functions[0].span;
        f.program.functions[0].parameters = vec![raw::Type::Stored(0)];
        let edge = |target| raw::Edge { target, arguments: vec![] };
        let mut blocks = vec![raw::Block {
            id: 0,
            parameters: vec![definition(0, raw::Type::Stored(0))],
            instructions: vec![],
            span,
            terminator: raw::Terminator::Jump(edge(1)),
        }];
        for header in 1..=depth {
            blocks.push(raw::Block {
                id: header,
                parameters: vec![],
                instructions: vec![],
                span,
                terminator: raw::Terminator::Branch {
                    condition: 0,
                    yes: edge(if header == depth { depth + header } else { header + 1 }),
                    no: edge(if header == 1 { 2 * depth + 1 } else { depth + header - 1 }),
                },
            });
        }
        for header in 1..=depth {
            blocks.push(raw::Block {
                id: depth + header,
                parameters: vec![],
                instructions: vec![],
                span,
                terminator: raw::Terminator::Jump(edge(header)),
            });
        }
        blocks.push(raw::Block {
            id: 2 * depth + 1,
            parameters: vec![],
            instructions: vec![raw::Instruction {
                result: definition(1, raw::Type::Stored(1)),
                span,
                operation: raw::Operation::I32Literal(0),
            }],
            span,
            terminator: raw::Terminator::Return(1),
        });
        f.program.functions[0].blocks = blocks;
        f
    }
    nested(128).check().expect("exact nested reducible loops");
    rejection(nested(129).check(), "ZRYNA-I3201");
    nested(128).check().expect("pristine replay");
}

#[test]
fn inherited_block_exact_first_extra_and_pristine_replay() {
    fn chain(count: u32) -> Fixture {
        let mut f = Fixture::new(false);
        let span = f.program.functions[0].span;
        f.program.functions[0].blocks = (0..count)
            .map(|id| {
                let last = id + 1 == count;
                raw::Block {
                    id,
                    parameters: vec![],
                    instructions: if last {
                        vec![raw::Instruction {
                            result: definition(0, raw::Type::Stored(1)),
                            span,
                            operation: raw::Operation::I32Literal(0),
                        }]
                    } else {
                        vec![]
                    },
                    span,
                    terminator: if last {
                        raw::Terminator::Return(0)
                    } else {
                        raw::Terminator::Jump(raw::Edge { target: id + 1, arguments: vec![] })
                    },
                }
            })
            .collect();
        f
    }
    chain(4096).check().expect("exact inherited block arena");
    rejection(chain(4097).check(), "ZRYNA-I3201");
    chain(4096).check().expect("pristine replay");
}
