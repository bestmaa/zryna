//! Genuine complete syntax authority with independently authored closed graph claims.

use super::super::{raw, validate_source_graph};
use super::fixtures::{Fixture, definition, key, rejection};
use zryna_layout::{
    StorageTarget,
    generic_v1::{self as layout, raw as types},
};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::v5::{VerifiedProjectSyntaxV5, decode_snapshot, verify_snapshot};

const MAIN: &str = include_str!("../../../../../tests/m7-syntax-fixtures/main.zry");
const VALUES: &str = include_str!("../../../../../tests/m7-syntax-fixtures/values.zry");
const SNAPSHOT: &[u8] = include_bytes!("../../../../../tests/m7-syntax-fixtures/reference.json");

struct Authenticated {
    fixture: Fixture,
    syntax: VerifiedProjectSyntaxV5,
    graph: types::Graph,
}

impl Authenticated {
    fn new() -> Self {
        let sources = SourceMap::build(vec![
            SourceFileInput { path: "main.zry".into(), text: MAIN.into() },
            SourceFileInput { path: "values.zry".into(), text: VALUES.into() },
        ])
        .expect("original exact UTF-8 source");
        let syntax =
            verify_snapshot(decode_snapshot(SNAPSHOT).expect("independent frozen DTO"), &sources)
                .expect("complete source/arena authentication");
        let graph = graph(&sources, &syntax);
        let mut fixture = Fixture::from_graph(sources, &graph);
        fixture.program.modules =
            vec![raw::Module { id: 0, functions: 1 }, raw::Module { id: 1, functions: 2 }];
        fixture.program.declarations = syntax
            .files()
            .iter()
            .flat_map(|unit| {
                unit.functions.iter().enumerate().map(|(index, function)| raw::Declaration {
                    module: unit.id,
                    function: u32::try_from(index).expect("index"),
                    parameters: u32::try_from(
                        function.type_parameters.as_ref().map_or(0, |list| list.parameters.len()),
                    )
                    .expect("arity"),
                    span: function.span,
                })
            })
            .collect();
        let mut functions = closed_templates(&syntax);
        let mut root = fixture.program.functions.remove(0);
        root.span = syntax.files()[0].functions[0].span;
        root.blocks[0].span = root.span;
        root.blocks[0].instructions[0].span = root.span;
        root.public_export = Some("score".into());
        functions.push(root);
        fixture.program.functions = functions;
        Self { fixture, syntax, graph }
    }

    fn check(&self) -> Result<(), super::super::Failure> {
        validate_source_graph(
            &self.fixture.program,
            &self.syntax,
            &self.fixture.sources,
            &self.fixture.linear,
            &self.fixture.linux,
        )
    }

    fn relayout(&mut self) {
        self.fixture.linear =
            layout::verify(&self.graph, &self.fixture.sources, StorageTarget::Linear32V1)
                .expect("independently layout-valid claim");
        self.fixture.linux =
            layout::verify(&self.graph, &self.fixture.sources, StorageTarget::LinuxX8664V1)
                .expect("independently layout-valid claim");
        self.fixture.program.type_keys =
            self.fixture.linear.types().map(|ty| ty.key().to_vec()).collect();
        self.fixture.program.universe = *self.fixture.linear.universe_identity();
        self.fixture.program.linear32 = *self.fixture.linear.fingerprint();
        self.fixture.program.linux_x86_64 = *self.fixture.linux.fingerprint();
    }
}

fn graph(sources: &SourceMap, syntax: &VerifiedProjectSyntaxV5) -> types::Graph {
    let modules = syntax
        .files()
        .iter()
        .map(|unit| types::Module {
            id: types::ModuleId(unit.id),
            source_file: sources.verify_file_id(unit.id).expect("file"),
            data_declarations: u32::try_from(unit.data_declarations.len()).expect("count"),
        })
        .collect();
    let declarations = syntax.files()[1]
        .data_declarations
        .iter()
        .enumerate()
        .map(|(index, data)| types::Declaration {
            module: types::ModuleId(1),
            index: u32::try_from(index).expect("index"),
            kind: if index == 0 { types::NominalKind::Struct } else { types::NominalKind::Enum },
            parameters: if index == 0 { 1 } else { 2 },
            members: if index == 0 { 1 } else { 3 },
            span: sources.verify_span(data.span).expect("bound source declaration"),
        })
        .collect::<Vec<_>>();
    let mut nodes = [
        zryna_layout::raw::TypeKind::Bool,
        zryna_layout::raw::TypeKind::I32,
        zryna_layout::raw::TypeKind::String,
    ]
    .into_iter()
    .enumerate()
    .map(|(id, kind)| types::TypeNode {
        id: types::NodeId(u32::try_from(id).expect("index")),
        span: None,
        kind: types::TypeKind::Base(kind),
    })
    .collect::<Vec<_>>();
    nodes.extend([
        types::TypeNode {
            id: types::NodeId(3),
            span: Some(declarations[0].span),
            kind: types::TypeKind::Struct {
                module: types::ModuleId(1),
                declaration: 0,
                arguments: vec![types::NodeId(1)],
                fields: vec![types::Field { ordinal: 0, ty: types::NodeId(1) }],
            },
        },
        types::TypeNode {
            id: types::NodeId(4),
            span: Some(declarations[1].span),
            kind: types::TypeKind::Enum {
                module: types::ModuleId(1),
                declaration: 1,
                arguments: vec![types::NodeId(1), types::NodeId(2)],
                variants: vec![
                    types::Variant { ordinal: 0, payload: Some(types::NodeId(1)) },
                    types::Variant { ordinal: 1, payload: Some(types::NodeId(2)) },
                    types::Variant { ordinal: 2, payload: None },
                ],
            },
        },
        types::TypeNode {
            id: types::NodeId(5),
            span: None,
            kind: types::TypeKind::Option { argument: types::NodeId(1) },
        },
        types::TypeNode {
            id: types::NodeId(6),
            span: None,
            kind: types::TypeKind::Result { okay: types::NodeId(1), error: types::NodeId(2) },
        },
    ]);
    types::Graph {
        modules,
        declarations,
        types: nodes,
        program_roots: (0..7).map(types::NodeId).collect(),
    }
}

fn closed_templates(syntax: &VerifiedProjectSyntaxV5) -> Vec<raw::Function> {
    let mut functions = [0u8, 1]
        .into_iter()
        .map(|primitive| {
            let span = syntax.files()[1].functions[0].span;
            let ty = raw::Type::Stored(u32::from(primitive));
            raw::Function {
                key: key(0x40, 1, 0, &[&[primitive]]),
                span,
                public_export: None,
                parameters: vec![ty],
                result: ty,
                blocks: vec![raw::Block {
                    id: 0,
                    parameters: vec![definition(0, ty)],
                    instructions: vec![],
                    span,
                    terminator: raw::Terminator::Return(0),
                }],
            }
        })
        .collect::<Vec<_>>();
    let span = syntax.files()[1].functions[1].span;
    functions.push(raw::Function {
        key: key(0x40, 1, 1, &[&[0], &[1]]),
        span,
        public_export: None,
        parameters: vec![raw::Type::Stored(0), raw::Type::Stored(1)],
        result: raw::Type::Stored(0),
        blocks: vec![raw::Block {
            id: 0,
            parameters: vec![
                definition(0, raw::Type::Stored(0)),
                definition(1, raw::Type::Stored(1)),
            ],
            instructions: vec![],
            span,
            terminator: raw::Terminator::Return(0),
        }],
    });
    functions
}

#[test]
fn original_signatures_and_nominal_members_are_independently_substituted() {
    Authenticated::new().check().expect("signature/layout stage, without executable authority");
}

#[test]
fn internally_consistent_forged_signature_cannot_replace_original_parameter() {
    let mut fixture = Authenticated::new();
    let function = &mut fixture.fixture.program.functions[1];
    function.parameters[0] = raw::Type::Stored(0);
    function.result = raw::Type::Stored(0);
    function.blocks[0].parameters[0].ty = raw::Type::Stored(0);
    fixture.fixture.check().expect("self-consistent typed graph alone");
    rejection(fixture.check(), "ZRYNA-I7001");
}

#[test]
fn original_spans_arities_exports_and_foreign_equal_text_authorities_reject() {
    for change in 0..4 {
        let mut fixture = Authenticated::new();
        match change {
            0 => {
                fixture.fixture.program.declarations[1].span.start += 1;
                fixture.fixture.program.functions[0].span.start += 1;
                fixture.fixture.program.functions[1].span.start += 1;
            }
            1 => fixture.fixture.program.declarations[1].parameters = 2,
            2 => fixture.fixture.program.functions[3].public_export = Some("other".into()),
            _ => {
                let foreign = Authenticated::new();
                fixture.syntax = foreign.syntax;
            }
        }
        rejection(fixture.check(), "ZRYNA-I7001");
    }
    Authenticated::new().check().expect("deterministic pristine replay");
}

#[test]
fn physically_valid_wrong_closed_field_and_inactive_variant_payload_reject() {
    for enumeration in [false, true] {
        let mut fixture = Authenticated::new();
        if enumeration {
            let types::TypeKind::Enum { variants, .. } = &mut fixture.graph.types[4].kind else {
                panic!("enum")
            };
            variants[1].payload = Some(types::NodeId(0));
        } else {
            let types::TypeKind::Struct { fields, .. } = &mut fixture.graph.types[3].kind else {
                panic!("struct")
            };
            fields[0].ty = types::NodeId(0);
        }
        fixture.relayout();
        fixture.fixture.check().expect("both physical layout seals and typed graph are valid");
        rejection(fixture.check(), "ZRYNA-I7001");
    }
}

#[test]
fn unused_original_data_metadata_cannot_be_omitted_or_rewritten() {
    let mut fixture = Authenticated::new();
    fixture.graph.types.remove(4);
    fixture.graph.types.remove(4);
    fixture.graph.types.remove(4);
    fixture.graph.program_roots = (0..4).map(types::NodeId).collect();
    fixture.graph.declarations[1].members = 2;
    fixture.relayout();
    fixture
        .fixture
        .check()
        .expect("unused original metadata is only an unauthenticated layout claim");
    rejection(fixture.check(), "ZRYNA-I7001");
}

fn claimed_call(fixture: &mut Authenticated) {
    let span = fixture.syntax.files()[0].functions[0]
        .body
        .expressions
        .iter()
        .find(|expression| {
            matches!(&expression.kind, zryna_syntax::v5::RawExpressionKind::Call { callee, .. }
            if callee.text == "identity")
        })
        .expect("authenticated original identity<i32> call")
        .span;
    let block = &mut fixture.fixture.program.functions[3].blocks[0];
    block.instructions.push(raw::Instruction {
        result: definition(1, raw::Type::Stored(1)),
        span,
        operation: raw::Operation::ClosedGenericCall { instance: 1, arguments: vec![0] },
    });
    block.terminator = raw::Terminator::Return(1);
}

#[test]
fn closed_call_keys_are_original_caller_substitution_not_merely_matching_signatures() {
    let mut fixture = Authenticated::new();
    claimed_call(&mut fixture);
    fixture.check().expect("authenticated imported original call target and explicit i32 argument");
    let block = &mut fixture.fixture.program.functions[3].blocks[0];
    block.instructions[0].result.ty = raw::Type::Stored(0);
    block.instructions[0].operation = raw::Operation::BoolLiteral(true);
    block.instructions[1].result.ty = raw::Type::Stored(0);
    block.instructions[1].operation =
        raw::Operation::ClosedGenericCall { instance: 0, arguments: vec![0] };
    block.instructions.push(raw::Instruction {
        result: definition(2, raw::Type::Stored(1)),
        span: block.span,
        operation: raw::Operation::I32Literal(7),
    });
    block.terminator = raw::Terminator::Return(2);
    fixture.fixture.check().expect("forged bool call is structurally and signature consistent");
    rejection(fixture.check(), "ZRYNA-I7001");
}

#[test]
fn call_with_an_invented_source_range_or_wrong_source_argument_count_rejects() {
    for range in [false, true] {
        let mut fixture = Authenticated::new();
        claimed_call(&mut fixture);
        let block = &mut fixture.fixture.program.functions[3].blocks[0];
        if range {
            block.instructions[1].span = block.span;
        } else {
            block.instructions[1].operation =
                raw::Operation::ClosedGenericCall { instance: 1, arguments: vec![] };
        }
        rejection(fixture.check(), "ZRYNA-I7001");
    }
    let mut pristine = Authenticated::new();
    claimed_call(&mut pristine);
    pristine.check().expect("pristine source-bound recovery");
}
