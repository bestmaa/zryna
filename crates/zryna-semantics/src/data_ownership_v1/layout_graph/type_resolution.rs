//! Exact storage-type interning and semantic type lookup.

use super::*;

#[derive(Default)]
pub(super) struct TypeInterners {
    arrays: BTreeMap<(u32, u64), raw_layout::NodeId>,
    vectors: BTreeMap<u32, raw_layout::NodeId>,
    shared: BTreeMap<u32, raw_layout::NodeId>,
    weak: BTreeMap<u32, raw_layout::NodeId>,
}

#[allow(clippy::too_many_lines)]
pub(super) fn resolve_graph_type(
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

pub(in super::super) fn semantic_type(
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
