//! External type roots and exact module-wide occurrence postorder.

use super::{
    DeclarationError, RawDataDeclarationKind, RawExpressionKind, RawSourceUnit, RawTypeSyntaxKind,
    arena,
    coverage::{Context, Cursor, Role},
};
use zryna_source::SourceMap;

fn children(kind: &RawTypeSyntaxKind) -> Vec<u32> {
    match kind {
        RawTypeSyntaxKind::Vec { argument, .. }
        | RawTypeSyntaxKind::Shared { argument, .. }
        | RawTypeSyntaxKind::Weak { argument, .. }
        | RawTypeSyntaxKind::Borrow { argument, .. }
        | RawTypeSyntaxKind::BorrowMut { argument, .. } => vec![*argument],
        RawTypeSyntaxKind::FixedArray { element, .. } => vec![*element],
        RawTypeSyntaxKind::Application { type_arguments, .. } => type_arguments.arguments.clone(),
        _ => Vec::new(),
    }
}

pub(super) fn validate(
    sources: &SourceMap,
    unit: &RawSourceUnit,
    context: Context,
) -> Result<(), DeclarationError> {
    // Missing annotations are source insertion points, never concrete type operands.
    for node in &unit.type_syntax {
        for child in children(&node.kind) {
            let child = unit.type_syntax.get(child as usize).ok_or_else(arena::malformed)?;
            if matches!(child.kind, RawTypeSyntaxKind::Missing) {
                return Err(arena::malformed());
            }
        }
    }
    let mut roots = Vec::new();
    for declaration in &unit.data_declarations {
        match &declaration.kind {
            RawDataDeclarationKind::Struct { fields, .. } => {
                roots.extend(fields.iter().map(|field| (field.type_syntax, false)));
            }
            RawDataDeclarationKind::Enum { variants, .. } => roots.extend(
                variants.iter().filter_map(|variant| variant.payload_type.map(|id| (id, false))),
            ),
        }
    }
    for function in &unit.functions {
        roots.extend(function.parameters.iter().map(|parameter| (parameter.type_syntax, true)));
        roots.push((function.result_type, true));
        for statement in &function.body.statements {
            if let crate::v4::RawStatementKind::LocalDeclaration { type_syntax, .. } =
                &statement.kind
            {
                roots.push((*type_syntax, true));
            }
        }
        for expression in &function.body.expressions {
            match &expression.kind {
                RawExpressionKind::VecConstruction { type_syntax, .. }
                | RawExpressionKind::FixedArrayConstruction { type_syntax, .. } => {
                    roots.push((*type_syntax, true));
                }
                RawExpressionKind::Call { type_arguments, .. }
                | RawExpressionKind::StructConstruction { type_arguments, .. }
                | RawExpressionKind::EnumConstruction { type_arguments, .. } => {
                    if let Some(arguments) = type_arguments {
                        roots.extend(arguments.arguments.iter().map(|id| (*id, true)));
                    }
                }
                _ => {}
            }
        }
    }
    let mut located = roots
        .into_iter()
        .map(|(id, in_function)| {
            unit.type_syntax
                .get(id as usize)
                .map(|node| (node.span.start, id, in_function))
                .ok_or_else(arena::malformed)
        })
        .collect::<Result<Vec<_>, _>>()?;
    located.sort_by_key(|(start, _, _)| *start);
    let roots = located.iter().map(|(_, id, _)| *id).collect::<Vec<_>>();
    arena::forest(unit.type_syntax.len(), &roots, |index| children(&unit.type_syntax[index].kind))?;
    for (_, root, in_function) in located {
        let context = if in_function { context.function() } else { context };
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let node = &unit.type_syntax[id as usize];
            let head = match &node.kind {
                RawTypeSyntaxKind::Named { name } => Some((name, Role::TypeHead)),
                RawTypeSyntaxKind::Application { name, .. } => Some((name, Role::ApplicationHead)),
                _ => None,
            };
            if let Some((name, role)) = head {
                let mut cursor = Cursor::new(sources, node.span)?;
                context.identifier(&mut cursor, name, role)?;
            }
            stack.extend(children(&node.kind));
        }
    }
    Ok(())
}
