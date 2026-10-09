use super::{
    Errors, MAX_FIXED_ARRAY_LENGTH, NormalizedSourcePath, RawTypeSyntax, RawTypeSyntaxKind,
    SourceMap, UntrustedSpan, checked_span, contains_claim, identifier, span_text, token,
};

pub(in crate::v4) fn own_type(
    id: u32,
    owners: &mut [u32],
    path: &NormalizedSourcePath,
    errors: &mut Errors,
) {
    match usize::try_from(id).ok().and_then(|i| owners.get_mut(i)) {
        Some(owner) => *owner = owner.saturating_add(1),
        None => errors.node(path, "type root references an unknown arena node"),
    }
}

pub(in crate::v4) fn verify_type_arena(
    raw: &[RawTypeSyntax],
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    owners: &mut [u32],
    errors: &mut Errors,
) -> Vec<u32> {
    let mut depths = vec![1u32; raw.len()];
    for (index, node) in raw.iter().enumerate() {
        checked_span(node.span, file, path, sources, errors, "type syntax");
        match &node.kind {
            RawTypeSyntaxKind::Missing => {
                if node.span.start != node.span.end {
                    errors.node(path, "missing type node must use an empty insertion span");
                }
            }
            RawTypeSyntaxKind::Named { name } => {
                identifier(name, file, path, sources, errors, "named type")
            }
            RawTypeSyntaxKind::String { keyword_span } => {
                token(*keyword_span, file, path, sources, errors, "String", "String type");
            }
            RawTypeSyntaxKind::Vec {
                keyword_span,
                less_than_span,
                argument,
                greater_than_span,
            } => type_container(
                index,
                "Vec",
                *keyword_span,
                *less_than_span,
                *argument,
                *greater_than_span,
                file,
                path,
                sources,
                owners,
                &mut depths,
                errors,
            ),
            RawTypeSyntaxKind::Shared {
                keyword_span,
                less_than_span,
                argument,
                greater_than_span,
            } => type_container(
                index,
                "Shared",
                *keyword_span,
                *less_than_span,
                *argument,
                *greater_than_span,
                file,
                path,
                sources,
                owners,
                &mut depths,
                errors,
            ),
            RawTypeSyntaxKind::Weak {
                keyword_span,
                less_than_span,
                argument,
                greater_than_span,
            } => type_container(
                index,
                "Weak",
                *keyword_span,
                *less_than_span,
                *argument,
                *greater_than_span,
                file,
                path,
                sources,
                owners,
                &mut depths,
                errors,
            ),
            RawTypeSyntaxKind::Borrow {
                keyword_span,
                less_than_span,
                argument,
                greater_than_span,
            } => type_container(
                index,
                "Borrow",
                *keyword_span,
                *less_than_span,
                *argument,
                *greater_than_span,
                file,
                path,
                sources,
                owners,
                &mut depths,
                errors,
            ),
            RawTypeSyntaxKind::BorrowMut {
                keyword_span,
                less_than_span,
                argument,
                greater_than_span,
            } => type_container(
                index,
                "BorrowMut",
                *keyword_span,
                *less_than_span,
                *argument,
                *greater_than_span,
                file,
                path,
                sources,
                owners,
                &mut depths,
                errors,
            ),
            RawTypeSyntaxKind::FixedArray {
                keyword_span,
                less_than_span,
                element,
                comma_span,
                length_span,
                length_spelling,
                length,
                greater_than_span,
            } => {
                token(*keyword_span, file, path, sources, errors, "FixedArray", "FixedArray type");
                token(*less_than_span, file, path, sources, errors, "<", "type open angle");
                token(*comma_span, file, path, sources, errors, ",", "type comma");
                token(*greater_than_span, file, path, sources, errors, ">", "type close angle");
                let verified_length_span =
                    checked_span(*length_span, file, path, sources, errors, "fixed-array length");
                if !canonical_u32(length_spelling)
                    || length_spelling.parse::<u32>().ok() != Some(*length)
                    || *length > MAX_FIXED_ARRAY_LENGTH
                    || verified_length_span.and_then(|span| span_text(span, sources))
                        != Some(length_spelling.as_str())
                {
                    errors.node(
                        path,
                        "fixed-array length is not a canonical source-authenticated u32",
                    );
                }
                type_edge(*element, index, owners, &mut depths, path, errors);
            }
        }
        let child = match node.kind {
            RawTypeSyntaxKind::Vec { argument, .. }
            | RawTypeSyntaxKind::Shared { argument, .. }
            | RawTypeSyntaxKind::Weak { argument, .. }
            | RawTypeSyntaxKind::Borrow { argument, .. }
            | RawTypeSyntaxKind::BorrowMut { argument, .. } => Some(argument),
            RawTypeSyntaxKind::FixedArray { element, .. } => Some(element),
            _ => None,
        };
        if let Some(child) =
            child.and_then(|id| usize::try_from(id).ok()).and_then(|id| raw.get(id))
            && !contains_claim(node.span, child.span)
        {
            errors.node(path, "type child span is outside its parent type span");
        }
    }
    depths
}

#[allow(clippy::too_many_arguments)]
pub(in crate::v4) fn type_container(
    index: usize,
    expected: &str,
    keyword: UntrustedSpan,
    less: UntrustedSpan,
    argument: u32,
    greater: UntrustedSpan,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    owners: &mut [u32],
    depths: &mut [u32],
    errors: &mut Errors,
) {
    token(keyword, file, path, sources, errors, expected, "container type keyword");
    token(less, file, path, sources, errors, "<", "type open angle");
    token(greater, file, path, sources, errors, ">", "type close angle");
    type_edge(argument, index, owners, depths, path, errors);
}

pub(in crate::v4) fn type_edge(
    raw: u32,
    parent: usize,
    owners: &mut [u32],
    depths: &mut [u32],
    path: &NormalizedSourcePath,
    errors: &mut Errors,
) {
    let Some(child) = usize::try_from(raw).ok() else {
        return;
    };
    if child >= parent {
        errors.node(path, "type edge is not canonical postorder");
        return;
    }
    let Some(owner) = owners.get_mut(child) else {
        errors.node(path, "type edge references an unknown node");
        return;
    };
    *owner = owner.saturating_add(1);
    depths[parent] = depths[parent].max(depths[child].saturating_add(1));
}

pub(in crate::v4) fn canonical_u32(value: &str) -> bool {
    value.len() <= 10
        && (value == "0" || (!value.starts_with('0') && value.bytes().all(|b| b.is_ascii_digit())))
        && value.parse::<u32>().is_ok_and(|length| length <= MAX_FIXED_ARRAY_LENGTH)
}
