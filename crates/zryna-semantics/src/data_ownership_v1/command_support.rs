use zryna_diagnostics::Diagnostic;
use zryna_layout::raw as layout;
use zryna_source::{SourceMap, Span};
use zryna_syntax::{command_h1_v1::CommandSyntax, v4::RawTypeSyntaxKind};

use super::{SemanticInput, Ty, layout_graph::Decl};

pub(crate) fn lower(
    source: &CommandSyntax,
    sources: &SourceMap,
) -> Result<
    (
        zryna_ir::command_h1_v1::VerifiedProgram,
        zryna_ownership_runtime_abi::VerifiedOwnershipRuntimeAbi,
    ),
    Vec<Diagnostic>,
> {
    let source = zryna_syntax::command_h1_v1::admit(source.syntax(), sources)
        .map_err(|error| vec![error])?;
    let entry = source.syntax().files()[0].id();
    let Some(mut input) = SemanticInput::try_new(source.syntax(), sources, entry) else {
        return Err(vec![Diagnostic::error(
            "ZRYNA-M4100",
            None,
            "command source is not bound to the final semantic input",
            "admit complete source through the command source boundary",
        )]);
    };
    input.command = Some(&source);
    let candidate = super::lowering::candidate(input)?;
    let ir = zryna_ir::command_h1_v1::verify(
        candidate.program,
        sources,
        &source,
        candidate.linear,
        candidate.linux,
    )?;
    Ok((ir, candidate.runtime_abi))
}

pub(super) fn builtin_span(input: SemanticInput<'_>) -> Option<Span> {
    let command = input.command?;
    command
        .environment()
        .map(zryna_syntax::command_h1_v1::EnvironmentRequirement::call_span)
        .or_else(|| {
            command.syntax().files()[0].type_syntax().iter().find_map(|ty| {
                matches!(&ty.kind, RawTypeSyntaxKind::Named { name } if name.text == "EnvLookupV1")
                    .then(|| super::span(input.sources(), ty.span))
            })
        })
}

pub(super) fn append_builtin(
    input: SemanticInput<'_>,
    graph: &mut layout::Graph,
    declarations: &mut Vec<Decl>,
) {
    let Some(span) = builtin_span(input) else {
        return;
    };
    let declaration = declarations.len();
    let node = layout::NodeId(u32::try_from(graph.types.len()).expect("bounded graph"));
    graph.modules[0].data_declarations += 1;
    graph.types.push(layout::TypeNode {
        id: node,
        span: Some(span),
        kind: layout::TypeKind::Enum {
            module: layout::ModuleId(0),
            declaration: u32::try_from(declaration).expect("bounded declarations"),
            variants: vec![
                layout::Variant { ordinal: 0, payload: Some(layout::NodeId(2)) },
                layout::Variant { ordinal: 1, payload: None },
            ],
        },
    });
    declarations.push(Decl { module: 0, declaration, name: "EnvLookupV1".into(), node, span });
    graph.program_roots.push(node);
}

pub(super) fn builtin_type(
    input: SemanticInput<'_>,
    declarations: &[Decl],
    node_types: &[Option<Ty>],
) -> Option<Ty> {
    input.command?;
    let declaration = declarations
        .iter()
        .find(|declaration| declaration.name == "EnvLookupV1" && declaration.module == 0)?;
    node_types.get(declaration.node.0 as usize).copied().flatten()
}

pub(super) fn environment_key(
    input: SemanticInput<'_>,
    function: &super::syntax::RawFunctionSyntax,
    expression: u32,
) -> Option<String> {
    let requirement = input.command?.environment()?;
    let file = input.syntax().files().first()?;
    let source_function = file.functions().get(requirement.function_index())?;
    (source_function.span == function.span && requirement.expression_index() == expression as usize)
        .then(|| requirement.key().to_owned())
}
