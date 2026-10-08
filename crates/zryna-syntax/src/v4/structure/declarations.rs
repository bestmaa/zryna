use super::{
    Errors, NormalizedSourcePath, RawDataDeclaration, RawDataDeclarationKind, RawTypeSyntax,
    RawTypeSyntaxKind, check_sequence, checked_index,
};

pub(in crate::v4) fn verify_type_structure(
    node: &RawTypeSyntax,
    _: usize,
    types: &[RawTypeSyntax],
    path: &NormalizedSourcePath,
    errors: &mut Errors,
) {
    let children = match &node.kind {
        RawTypeSyntaxKind::Missing => Vec::new(),
        RawTypeSyntaxKind::Named { name } => vec![name.span],
        RawTypeSyntaxKind::String { keyword_span } => vec![*keyword_span],
        RawTypeSyntaxKind::Vec { keyword_span, less_than_span, argument, greater_than_span }
        | RawTypeSyntaxKind::Shared { keyword_span, less_than_span, argument, greater_than_span }
        | RawTypeSyntaxKind::Weak { keyword_span, less_than_span, argument, greater_than_span }
        | RawTypeSyntaxKind::Borrow { keyword_span, less_than_span, argument, greater_than_span }
        | RawTypeSyntaxKind::BorrowMut {
            keyword_span,
            less_than_span,
            argument,
            greater_than_span,
        } => checked_index(types, *argument).map_or_else(Vec::new, |argument| {
            vec![*keyword_span, *less_than_span, argument.span, *greater_than_span]
        }),
        RawTypeSyntaxKind::FixedArray {
            keyword_span,
            less_than_span,
            element,
            comma_span,
            length_span,
            greater_than_span,
            ..
        } => checked_index(types, *element).map_or_else(Vec::new, |element| {
            vec![
                *keyword_span,
                *less_than_span,
                element.span,
                *comma_span,
                *length_span,
                *greater_than_span,
            ]
        }),
    };
    check_sequence(node.span, &children, path, errors, "type syntax children");
}

pub(in crate::v4) fn verify_declaration_structure(
    raw: &RawDataDeclaration,
    types: &[RawTypeSyntax],
    path: &NormalizedSourcePath,
    errors: &mut Errors,
) {
    let mut children = raw.export_span.into_iter().collect::<Vec<_>>();
    match &raw.kind {
        RawDataDeclarationKind::Struct {
            interface_span,
            name,
            extends_span,
            marker_span,
            open_brace_span,
            fields,
            close_brace_span,
        } => {
            children.extend([
                *interface_span,
                name.span,
                *extends_span,
                *marker_span,
                *open_brace_span,
            ]);
            for field in fields {
                let mut member = vec![field.name.span, field.colon_span];
                if let Some(ty) = checked_index(types, field.type_syntax) {
                    member.push(ty.span);
                }
                member.push(field.semicolon_span);
                check_sequence(field.span, &member, path, errors, "struct field children");
                children.push(field.span);
            }
            children.push(*close_brace_span);
        }
        RawDataDeclarationKind::Enum {
            interface_span,
            name,
            extends_span,
            marker_span,
            open_brace_span,
            variants,
            close_brace_span,
        } => {
            children.extend([
                *interface_span,
                name.span,
                *extends_span,
                *marker_span,
                *open_brace_span,
            ]);
            for variant in variants {
                let mut member = vec![variant.name.span, variant.colon_span];
                if let Some(id) = variant.payload_type
                    && let Some(ty) = checked_index(types, id)
                {
                    member.push(ty.span);
                }
                if let Some(span) = variant.none_span {
                    member.push(span);
                }
                member.push(variant.semicolon_span);
                check_sequence(variant.span, &member, path, errors, "enum variant children");
                children.push(variant.span);
            }
            children.push(*close_brace_span);
        }
    }
    check_sequence(raw.span, &children, path, errors, "data declaration children");
}
