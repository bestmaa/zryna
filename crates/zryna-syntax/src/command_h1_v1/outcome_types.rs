//! Authenticate reserved outcome annotations from exact source punctuation and v4 ownership.

use std::collections::BTreeSet;

use crate::v4::{
    RawDataDeclarationKind, RawExpressionKind, RawStatementKind, RawTypeSyntax, RawTypeSyntaxKind,
    SourceUnit,
};

use super::{OUTCOME, identifiers};

pub(super) fn positions(
    file: &SourceUnit,
    source: &str,
) -> Result<BTreeSet<(u32, u32)>, &'static str> {
    let types = file.type_syntax();
    let parents = type_parents(types)?;
    let mut roots = BTreeSet::new();
    let mut annotation = |id: u32, after: u32, punctuation: &[u8]| {
        let index = usize::try_from(id).map_err(|_| "invalid type")?;
        let ty = types.get(index).ok_or("invalid type")?;
        if between(source, after, ty.span.start, punctuation).is_ok() {
            roots.insert(index);
        }
        Ok::<(), &'static str>(())
    };
    for declaration in file.data_declarations() {
        match &declaration.kind {
            RawDataDeclarationKind::Struct { fields, .. } => {
                for field in fields {
                    annotation(field.type_syntax, field.name.span.end, b":")?;
                }
            }
            RawDataDeclarationKind::Enum { variants, .. } => {
                for variant in variants {
                    if let Some(id) = variant.payload_type {
                        annotation(id, variant.name.span.end, b":")?;
                    }
                }
            }
        }
    }
    for function in file.functions() {
        if between(source, function.function_span.end, function.name.span.start, b"").is_err() {
            continue;
        }
        for parameter in &function.parameters {
            annotation(parameter.type_syntax, parameter.name.span.end, b":")?;
        }
        if let Some(last) = function.parameters.last() {
            annotation(function.result_type, last.span.end, b"):")?;
            annotation(function.result_type, last.span.end, b",):")?;
        } else {
            annotation(function.result_type, function.name.span.end, b"():")?;
        }
        for statement in &function.body.statements {
            if let RawStatementKind::LocalDeclaration { keyword_span, name, type_syntax, .. } =
                &statement.kind
                && between(source, keyword_span.end, name.span.start, b"").is_ok()
            {
                annotation(*type_syntax, name.span.end, b":")?;
            }
        }
    }
    // Container constructors begin with their exact source type. V4 independently owns the
    // referenced type and authenticates its source tokens and ordering, including generic brackets.
    for function in file.functions() {
        for expression in &function.body.expressions {
            let id = match expression.kind {
                RawExpressionKind::FixedArrayConstruction { type_syntax, .. }
                | RawExpressionKind::VecConstruction { type_syntax, .. } => Some(type_syntax),
                _ => None,
            };
            if let Some(id) = id {
                let index = usize::try_from(id).map_err(|_| "invalid type")?;
                if types.get(index).is_some_and(|ty| ty.span.start == expression.span.start) {
                    roots.insert(index);
                }
            }
        }
    }
    let mut result = BTreeSet::new();
    for (index, ty) in types.iter().enumerate() {
        let RawTypeSyntaxKind::Named { name } = &ty.kind else {
            continue;
        };
        if name.text != OUTCOME {
            continue;
        }
        let mut root = index;
        let mut depth = 0;
        while let Some(parent) = parents[root] {
            depth += 1;
            if depth > 128 {
                return Err("reserved outcome type exceeds its depth bound");
            }
            container_gap(source, &types[parent], &types[root])?;
            root = parent;
        }
        if ty.span != name.span {
            return Err("reserved outcome type span differs from its token");
        }
        if !roots.contains(&root) {
            return Err("reserved outcome does not have an exact source annotation context");
        }
        result.insert((name.span.start, name.span.end));
    }
    Ok(result)
}

fn type_parents(types: &[RawTypeSyntax]) -> Result<Vec<Option<usize>>, &'static str> {
    let mut parents = vec![None; types.len()];
    for (index, ty) in types.iter().enumerate() {
        let child = match ty.kind {
            RawTypeSyntaxKind::Vec { argument, .. }
            | RawTypeSyntaxKind::Shared { argument, .. }
            | RawTypeSyntaxKind::Weak { argument, .. }
            | RawTypeSyntaxKind::Borrow { argument, .. }
            | RawTypeSyntaxKind::BorrowMut { argument, .. } => Some(argument),
            RawTypeSyntaxKind::FixedArray { element, .. } => Some(element),
            _ => None,
        };
        if let Some(child) = child {
            let slot = parents
                .get_mut(usize::try_from(child).map_err(|_| "invalid type")?)
                .ok_or("invalid type")?;
            *slot = Some(index);
        }
    }
    Ok(parents)
}

fn container_gap(
    source: &str,
    parent: &RawTypeSyntax,
    child: &RawTypeSyntax,
) -> Result<(), &'static str> {
    let (keyword, length) = match &parent.kind {
        RawTypeSyntaxKind::Vec { keyword_span, .. }
        | RawTypeSyntaxKind::Shared { keyword_span, .. }
        | RawTypeSyntaxKind::Weak { keyword_span, .. }
        | RawTypeSyntaxKind::Borrow { keyword_span, .. }
        | RawTypeSyntaxKind::BorrowMut { keyword_span, .. } => (*keyword_span, None),
        RawTypeSyntaxKind::FixedArray { keyword_span, length_span, .. } => {
            (*keyword_span, Some(*length_span))
        }
        _ => return Err("invalid reserved outcome parent type"),
    };
    if parent.span.start != keyword.start {
        return Err("reserved outcome container has omitted leading source syntax");
    }
    between(source, keyword.end, child.span.start, b"<")?;
    if let Some(length) = length {
        between(source, child.span.end, length.start, b",")?;
        between(source, length.end, parent.span.end, b">")
    } else {
        between(source, child.span.end, parent.span.end, b">")
    }
}

fn between(source: &str, after: u32, before: u32, tokens: &[u8]) -> Result<(), &'static str> {
    let after = usize::try_from(after).map_err(|_| "invalid annotation")?;
    let before = usize::try_from(before).map_err(|_| "invalid annotation")?;
    let gap = source.as_bytes().get(after..before).ok_or("invalid annotation")?;
    let mut cursor = 0;
    for token in tokens {
        identifiers::trivia(gap, &mut cursor)?;
        if gap.get(cursor) != Some(token) {
            return Err("annotation punctuation differs");
        }
        cursor += 1;
    }
    identifiers::trivia(gap, &mut cursor)?;
    if cursor == gap.len() { Ok(()) } else { Err("annotation contains omitted source syntax") }
}
