use super::super::{VerifiedProgram, raw, verify};
use zryna_diagnostics::Diagnostic;
use zryna_layout::{StorageTarget, VerifiedLayouts, raw as layout};
use zryna_source::{SourceFileInput, SourceMap, Span};
use zryna_syntax::{command_h1_v1::CommandSyntax, v4};

pub(super) struct Fixture {
    pub(super) sources: SourceMap,
    pub(super) source: CommandSyntax,
    pub(super) linear: VerifiedLayouts,
    pub(super) linux: VerifiedLayouts,
    pub(super) graph: layout::Graph,
    pub(super) span: Span,
    pub(super) call: Span,
    pub(super) matched: Span,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let text =
            include_str!("../../../../../tests/wasi-command-source-fixtures/environment-match.zry");
        let sources = SourceMap::build(vec![SourceFileInput {
            path: "src/main.zry".into(),
            text: text.into(),
        }])
        .expect("source map");
        let snapshot = v4::decode_snapshot(include_bytes!(
            "../../../../../tests/wasi-command-source-fixtures/environment-match.json"
        ))
        .expect("independent source fixture");
        let syntax = v4::verify_snapshot(snapshot, &sources).expect("authenticated source");
        let source =
            zryna_syntax::command_h1_v1::admit(&syntax, &sources).expect("complete command source");
        let function = &source.syntax().files()[0].functions()[0];
        let span = sources.verify_span(function.span).expect("function span");
        let call = source.environment().expect("environment").call_span();
        let matched = function
            .body
            .expressions
            .iter()
            .find_map(|expression| {
                matches!(expression.kind, v4::RawExpressionKind::Match { .. })
                    .then(|| sources.verify_span(expression.span).expect("match span"))
            })
            .expect("source match");
        let file = sources.verify_file_id(0).expect("file");
        let graph = layout::Graph {
            modules: vec![layout::Module {
                id: layout::ModuleId(0),
                source_file: file,
                data_declarations: 1,
            }],
            types: [
                layout::TypeKind::Bool,
                layout::TypeKind::I32,
                layout::TypeKind::String,
                layout::TypeKind::Enum {
                    module: layout::ModuleId(0),
                    declaration: 0,
                    variants: vec![
                        layout::Variant { ordinal: 0, payload: Some(layout::NodeId(2)) },
                        layout::Variant { ordinal: 1, payload: None },
                    ],
                },
            ]
            .into_iter()
            .enumerate()
            .map(|(index, kind)| layout::TypeNode {
                id: layout::NodeId(u32::try_from(index).expect("four types")),
                span: (index == 3).then_some(call),
                kind,
            })
            .collect(),
            program_roots: vec![layout::NodeId(0), layout::NodeId(3)],
        };
        let (linear, linux) = Self::layouts(&graph, &sources);
        Self { sources, source, linear, linux, graph, span, call, matched }
    }

    pub(super) fn layouts(
        graph: &layout::Graph,
        sources: &SourceMap,
    ) -> (VerifiedLayouts, VerifiedLayouts) {
        (
            zryna_layout::verify(graph, sources, StorageTarget::Linear32V1).expect("linear layout"),
            zryna_layout::verify(graph, sources, StorageTarget::LinuxX8664V1)
                .expect("native layout"),
        )
    }

    pub(super) fn check(&self, program: raw::Program) -> Result<VerifiedProgram, Vec<Diagnostic>> {
        verify(program, &self.sources, &self.source, self.linear.clone(), self.linux.clone())
    }

    pub(super) fn seed(&self) -> raw::Program {
        let string = raw::TypeId(2);
        let outcome = raw::TypeId(3);
        let boolean = raw::TypeId(0);
        let place = |id, ty, kind| raw::Place { id: raw::PlaceId(id), ty, span: self.span, kind };
        raw::Program {
            authorities: raw::AuthorityClaims {
                runtime: crate::data_ownership_v1::RuntimeContractIdentity::CommandH1V1,
                type_universe: self.linear.universe_identity().as_bytes(),
                linear32_fingerprint: *self.linear.fingerprint(),
                linux_x86_64_fingerprint: *self.linux.fingerprint(),
            },
            entry_module: raw::ModuleId(0),
            modules: vec![raw::Module {
                id: raw::ModuleId(0),
                source_file: self.sources.verify_file_id(0).expect("file"),
                data_declarations: 1,
                functions: vec![raw::Function {
                    id: raw::FunctionId { module: raw::ModuleId(0), declaration: 0 },
                    entry_export: Some("main".into()),
                    span: self.span,
                    parameters: vec![],
                    borrow_parameters: vec![],
                    result: boolean,
                    places: vec![
                        place(0, outcome, raw::PlaceKind::Temporary(raw::ValueId(0))),
                        place(
                            1,
                            string,
                            raw::PlaceKind::EnumPayload { base: raw::PlaceId(0), variant: 0 },
                        ),
                        place(2, string, raw::PlaceKind::Temporary(raw::ValueId(1))),
                    ],
                    blocks: self.blocks(),
                    cleanup_plans: (0..3)
                        .map(|id| raw::CleanupPlan {
                            id: raw::CleanupPlanId(id),
                            span: if id == 0 { self.call } else { self.span },
                            actions: vec![],
                        })
                        .collect(),
                }],
            }],
        }
    }

    fn blocks(&self) -> Vec<raw::Block> {
        let cleanup = raw::CleanupPlanId(0);
        let string = raw::TypeId(2);
        let outcome = raw::TypeId(3);
        let boolean = raw::TypeId(0);
        let value = |id, ty, span| raw::ValueDefinition { id: raw::ValueId(id), ty, span };
        let instruction = |result, kind| raw::Instruction { result, span: self.span, kind };
        let finish = |id, block, truth, mut instructions: Vec<raw::Instruction>| {
            instructions.push(instruction(
                Some(value(id, boolean, self.span)),
                raw::InstructionKind::BoolLiteral(truth),
            ));
            raw::Block {
                id: raw::BlockId(block),
                parameters: vec![],
                instructions,
                terminators: vec![raw::SpannedTerminator {
                    span: self.span,
                    kind: raw::Terminator::Return {
                        value: raw::ValueId(id),
                        cleanup: raw::CleanupPlanId(block),
                    },
                }],
            }
        };
        vec![
            raw::Block {
                id: raw::BlockId(0),
                parameters: vec![],
                instructions: vec![raw::Instruction {
                    result: Some(value(0, outcome, self.call)),
                    span: self.call,
                    kind: raw::InstructionKind::EnvironmentLookup { key: "MODE".into(), cleanup },
                }],
                terminators: vec![raw::SpannedTerminator {
                    span: self.matched,
                    kind: raw::Terminator::EnumMatch {
                        place: raw::PlaceId(0),
                        arms: (0..2)
                            .map(|variant| raw::EnumArm {
                                variant,
                                edge: raw::Edge {
                                    target: raw::BlockId(variant + 1),
                                    arguments: vec![],
                                },
                            })
                            .collect(),
                    },
                }],
            },
            finish(
                2,
                1,
                true,
                vec![
                    instruction(
                        Some(value(1, string, self.span)),
                        raw::InstructionKind::MoveFromPlace { place: raw::PlaceId(1) },
                    ),
                    instruction(None, raw::InstructionKind::DropPlace { place: raw::PlaceId(2) }),
                    instruction(None, raw::InstructionKind::DropPlace { place: raw::PlaceId(0) }),
                ],
            ),
            finish(
                3,
                2,
                false,
                vec![instruction(None, raw::InstructionKind::DropPlace { place: raw::PlaceId(0) })],
            ),
        ]
    }
}
