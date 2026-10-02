use std::collections::BTreeMap;

use zryna_layout::raw as raw_layout;
use zryna_source::{SourceMap, Span, UntrustedSpan};
use zryna_syntax::v4::{
    self as syntax, RawDataDeclarationKind, RawExpressionKind, RawStatementKind, RawTypeSyntaxKind,
};

use super::SemanticInput;
use super::diagnostics::Errors;
use super::type_model::Ty;
use crate::data_ownership_v1::diagnostics::span;

#[derive(Clone)]
pub(super) struct Decl {
    pub(super) module: usize,
    pub(super) declaration: usize,
    pub(super) name: String,
    pub(super) node: raw_layout::NodeId,
    pub(super) span: Span,
}

#[derive(Default)]
struct TypeInterners {
    arrays: BTreeMap<(u32, u64), raw_layout::NodeId>,
    vectors: BTreeMap<u32, raw_layout::NodeId>,
    shared: BTreeMap<u32, raw_layout::NodeId>,
    weak: BTreeMap<u32, raw_layout::NodeId>,
}

fn storage_type_syntax(file: &syntax::SourceUnit, mut id: u32) -> u32 {
    loop {
        let Some(ty) = usize::try_from(id).ok().and_then(|index| file.type_syntax().get(index))
        else {
            return id;
        };
        match ty.kind {
            RawTypeSyntaxKind::Borrow { argument, .. }
            | RawTypeSyntaxKind::BorrowMut { argument, .. } => id = argument,
            _ => return id,
        }
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn build_graph(
    input: SemanticInput<'_>,
    errors: &mut Errors<'_>,
) -> (raw_layout::Graph, Vec<Decl>) {
    let mut graph =
        raw_layout::Graph { modules: Vec::new(), types: Vec::new(), program_roots: Vec::new() };
    graph.types.push(raw_layout::TypeNode {
        id: raw_layout::NodeId(0),
        span: None,
        kind: raw_layout::TypeKind::Bool,
    });
    graph.types.push(raw_layout::TypeNode {
        id: raw_layout::NodeId(1),
        span: None,
        kind: raw_layout::TypeKind::I32,
    });
    graph.types.push(raw_layout::TypeNode {
        id: raw_layout::NodeId(2),
        span: None,
        kind: raw_layout::TypeKind::String,
    });
    let mut declarations = Vec::new();
    for (module, file) in input.syntax().files().iter().enumerate() {
        graph.modules.push(raw_layout::Module {
            id: raw_layout::ModuleId(u32::try_from(module).unwrap_or(u32::MAX)),
            source_file: file.id(),
            data_declarations: u32::try_from(file.data_declarations().len()).unwrap_or(u32::MAX),
        });
        let mut names = BTreeMap::new();
        for (declaration, value) in file.data_declarations().iter().enumerate() {
            let (name, name_span) = declaration_name(value, input.sources());
            if names.insert(name.to_ascii_lowercase(), name_span).is_some() {
                errors.at(
                    "ZRYNA-M3002",
                    name_span,
                    format!("aggregate type '{name}' is declared more than once"),
                    "give every aggregate declaration one exact module-local name",
                );
            }
            match &value.kind {
                RawDataDeclarationKind::Struct { fields, .. } => {
                    if fields.is_empty() {
                        errors.at(
                            "ZRYNA-M3004",
                            span(input.sources(), value.span),
                            "empty structs are outside aggregate M3",
                            "declare at least one Copy field",
                        );
                    }
                    verify_member_names(
                        fields.iter().map(|field| (&field.name.text, field.name.span)),
                        input.sources(),
                        "field",
                        errors,
                    );
                }
                RawDataDeclarationKind::Enum { variants, .. } => {
                    if variants.is_empty() {
                        errors.at(
                            "ZRYNA-M3004",
                            span(input.sources(), value.span),
                            "empty enums are outside aggregate M3",
                            "declare at least one enum variant",
                        );
                    }
                    verify_member_names(
                        variants.iter().map(|variant| (&variant.name.text, variant.name.span)),
                        input.sources(),
                        "variant",
                        errors,
                    );
                }
            }
            let node = raw_layout::NodeId(u32::try_from(graph.types.len()).unwrap_or(u32::MAX));
            graph.types.push(raw_layout::TypeNode {
                id: node,
                span: Some(span(input.sources(), value.span)),
                kind: raw_layout::TypeKind::Struct {
                    module: raw_layout::ModuleId(u32::try_from(module).unwrap_or(u32::MAX)),
                    declaration: u32::try_from(declaration).unwrap_or(u32::MAX),
                    fields: Vec::new(),
                },
            });
            declarations.push(Decl {
                module,
                declaration,
                name: name.to_owned(),
                node,
                span: span(input.sources(), value.span),
            });
        }
    }
    super::command_support::append_builtin(input, &mut graph, &mut declarations);
    let mut interners = TypeInterners::default();
    for module in 0..input.syntax().files().len() {
        let file = &input.syntax().files()[module];
        for declaration in 0..file.data_declarations().len() {
            let decl = declarations
                .iter()
                .find(|d| d.module == module && d.declaration == declaration)
                .expect("preallocated declaration");
            let kind = match &file.data_declarations()[declaration].kind {
                RawDataDeclarationKind::Struct { fields, .. } => raw_layout::TypeKind::Struct {
                    module: raw_layout::ModuleId(u32::try_from(module).unwrap_or(u32::MAX)),
                    declaration: u32::try_from(declaration).unwrap_or(u32::MAX),
                    fields: fields
                        .iter()
                        .enumerate()
                        .filter_map(|(ordinal, field)| {
                            resolve_graph_type(
                                file,
                                field.type_syntax,
                                module,
                                &declarations,
                                &mut graph,
                                &mut interners,
                                errors,
                            )
                            .map(|ty| raw_layout::Field {
                                ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
                                ty,
                            })
                        })
                        .collect(),
                },
                RawDataDeclarationKind::Enum { variants, .. } => raw_layout::TypeKind::Enum {
                    module: raw_layout::ModuleId(u32::try_from(module).unwrap_or(u32::MAX)),
                    declaration: u32::try_from(declaration).unwrap_or(u32::MAX),
                    variants: variants
                        .iter()
                        .enumerate()
                        .map(|(ordinal, variant)| raw_layout::Variant {
                            ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
                            payload: variant.payload_type.and_then(|id| {
                                resolve_graph_type(
                                    file,
                                    id,
                                    module,
                                    &declarations,
                                    &mut graph,
                                    &mut interners,
                                    errors,
                                )
                            }),
                        })
                        .collect(),
                },
            };
            graph.types[usize::try_from(decl.node.0).expect("node index")].kind = kind;
        }
        for function in file.functions() {
            for parameter in &function.parameters {
                add_root(
                    file,
                    storage_type_syntax(file, parameter.type_syntax),
                    module,
                    &declarations,
                    &mut graph,
                    &mut interners,
                    errors,
                );
            }
            add_root(
                file,
                storage_type_syntax(file, function.result_type),
                module,
                &declarations,
                &mut graph,
                &mut interners,
                errors,
            );
            for statement in &function.body.statements {
                if let RawStatementKind::LocalDeclaration { type_syntax, .. } = statement.kind {
                    let shared_referent = usize::try_from(type_syntax)
                        .ok()
                        .and_then(|index| file.type_syntax().get(index))
                        .and_then(|ty| match ty.kind {
                            RawTypeSyntaxKind::Borrow { argument, .. }
                            | RawTypeSyntaxKind::BorrowMut { argument, .. } => Some(argument),
                            _ => None,
                        });
                    add_root(
                        file,
                        shared_referent.unwrap_or(type_syntax),
                        module,
                        &declarations,
                        &mut graph,
                        &mut interners,
                        errors,
                    );
                }
            }
            for expression in &function.body.expressions {
                if let RawExpressionKind::FixedArrayConstruction { type_syntax, .. }
                | RawExpressionKind::VecConstruction { type_syntax, .. } = expression.kind
                {
                    add_root(
                        file,
                        type_syntax,
                        module,
                        &declarations,
                        &mut graph,
                        &mut interners,
                        errors,
                    );
                }
            }
        }
    }
    graph.program_roots.sort_by_key(|id| id.0);
    graph.program_roots.dedup();
    (graph, declarations)
}

fn add_root(
    file: &syntax::SourceUnit,
    id: u32,
    module: usize,
    declarations: &[Decl],
    graph: &mut raw_layout::Graph,
    interners: &mut TypeInterners,
    errors: &mut Errors<'_>,
) {
    if let Some(node) = resolve_graph_type(file, id, module, declarations, graph, interners, errors)
    {
        graph.program_roots.push(node);
    }
}

#[allow(clippy::too_many_lines)]
fn resolve_graph_type(
    file: &syntax::SourceUnit,
    id: u32,
    module: usize,
    declarations: &[Decl],
    graph: &mut raw_layout::Graph,
    interners: &mut TypeInterners,
    errors: &mut Errors<'_>,
) -> Option<raw_layout::NodeId> {
    let ty = usize::try_from(id).ok().and_then(|i| file.type_syntax().get(i))?;
    match &ty.kind {
        RawTypeSyntaxKind::Named { name } if name.text == "bool" => Some(raw_layout::NodeId(0)),
        RawTypeSyntaxKind::Named { name } if name.text == "i32" => Some(raw_layout::NodeId(1)),
        RawTypeSyntaxKind::String { .. } => Some(raw_layout::NodeId(2)),
        RawTypeSyntaxKind::Named { name } => declarations
            .iter()
            .find(|d| d.module == module && d.name == name.text)
            .map(|d| d.node)
            .or_else(|| {
                errors.at(
                    "ZRYNA-M3002",
                    span(errors.sources, name.span),
                    format!("type '{}' does not name a module-local aggregate", name.text),
                    "use bool, i32, or an exact aggregate declaration name",
                );
                None
            }),
        RawTypeSyntaxKind::FixedArray { element, length, .. } => {
            let element =
                resolve_graph_type(file, *element, module, declarations, graph, interners, errors)?;
            let length = u64::from(*length);
            if let Some(id) = interners.arrays.get(&(element.0, length)).copied() {
                return Some(id);
            }
            let id = raw_layout::NodeId(u32::try_from(graph.types.len()).ok()?);
            graph.types.push(raw_layout::TypeNode {
                id,
                span: None,
                kind: raw_layout::TypeKind::FixedArray { element, length },
            });
            interners.arrays.insert((element.0, length), id);
            Some(id)
        }
        RawTypeSyntaxKind::Vec { argument, .. } => {
            let element = resolve_graph_type(
                file,
                *argument,
                module,
                declarations,
                graph,
                interners,
                errors,
            )?;
            if let Some(id) = interners.vectors.get(&element.0).copied() {
                return Some(id);
            }
            let id = raw_layout::NodeId(u32::try_from(graph.types.len()).ok()?);
            graph.types.push(raw_layout::TypeNode {
                id,
                span: None,
                kind: raw_layout::TypeKind::Vec { element },
            });
            interners.vectors.insert(element.0, id);
            Some(id)
        }
        RawTypeSyntaxKind::Shared { argument, .. } | RawTypeSyntaxKind::Weak { argument, .. } => {
            let payload = resolve_graph_type(
                file,
                *argument,
                module,
                declarations,
                graph,
                interners,
                errors,
            )?;
            let (interner, shared) = match ty.kind {
                RawTypeSyntaxKind::Shared { .. } => (&mut interners.shared, true),
                RawTypeSyntaxKind::Weak { .. } => (&mut interners.weak, false),
                _ => unreachable!("matched handle type"),
            };
            if let Some(id) = interner.get(&payload.0).copied() {
                return Some(id);
            }
            let id = raw_layout::NodeId(u32::try_from(graph.types.len()).ok()?);
            graph.types.push(raw_layout::TypeNode {
                id,
                span: None,
                kind: if shared {
                    raw_layout::TypeKind::Shared { payload }
                } else {
                    raw_layout::TypeKind::Weak { payload }
                },
            });
            interner.insert(payload.0, id);
            Some(id)
        }
        RawTypeSyntaxKind::Missing => {
            errors.at(
                "ZRYNA-M3002",
                span(errors.sources, ty.span),
                "an exact aggregate type annotation is required",
                "write bool, i32, a Copy aggregate name, or FixedArray<T, N>",
            );
            None
        }
        _ => {
            errors.at(
                "ZRYNA-M3003",
                span(errors.sources, ty.span),
                "heap, handle, and borrow types are outside aggregate M3",
                "use only Copy bool, i32, structs, enums, and fixed arrays",
            );
            None
        }
    }
}

fn declaration_name<'a>(
    value: &'a syntax::RawDataDeclaration,
    sources: &SourceMap,
) -> (&'a str, Span) {
    let name = match &value.kind {
        RawDataDeclarationKind::Struct { name, .. } | RawDataDeclarationKind::Enum { name, .. } => {
            name
        }
    };
    (&name.text, span(sources, name.span))
}

fn verify_member_names<'a>(
    names: impl Iterator<Item = (&'a String, UntrustedSpan)>,
    sources: &SourceMap,
    label: &'static str,
    errors: &mut Errors<'_>,
) {
    let mut seen = BTreeMap::new();
    for (name, raw_span) in names {
        if seen.insert(name.to_ascii_lowercase(), raw_span).is_some() {
            errors.at(
                "ZRYNA-M3002",
                span(sources, raw_span),
                format!("{label} '{name}' collides under portable ASCII case folding"),
                "give every member a portable case-insensitive unique name",
            );
        }
    }
}

pub(super) fn semantic_type(
    file: &syntax::SourceUnit,
    id: u32,
    module: usize,
    declarations: &[Decl],
    graph: &raw_layout::Graph,
    node_types: &[Option<Ty>],
    errors: &mut Errors<'_>,
) -> Option<Ty> {
    let mut scratch = graph.clone();
    let mut interners = TypeInterners::default();
    for node in &graph.types {
        match node.kind {
            raw_layout::TypeKind::FixedArray { element, length } => {
                interners.arrays.insert((element.0, length), node.id);
            }
            raw_layout::TypeKind::Vec { element } => {
                interners.vectors.insert(element.0, node.id);
            }
            raw_layout::TypeKind::Shared { payload } => {
                interners.shared.insert(payload.0, node.id);
            }
            raw_layout::TypeKind::Weak { payload } => {
                interners.weak.insert(payload.0, node.id);
            }
            _ => {}
        }
    }
    let node =
        resolve_graph_type(file, id, module, declarations, &mut scratch, &mut interners, errors)?;
    node_types.get(usize::try_from(node.0).ok()?).and_then(|v| *v)
}
