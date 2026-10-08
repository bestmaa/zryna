use super::{
    BTreeSet, Errors, NormalizedSourcePath, RawDataDeclaration, RawDataDeclarationKind,
    RawIdentifierSyntax, SourceMap, checked_span, contains_claim, identifier, own_type, token,
};

pub(in crate::v4) fn verify_declaration(
    raw: &RawDataDeclaration,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    type_owners: &mut [u32],
    top_names: &mut BTreeSet<String>,
    errors: &mut Errors,
) {
    checked_span(raw.span, file, path, sources, errors, "data declaration");
    if let Some(span) = raw.export_span {
        token(span, file, path, sources, errors, "export", "export keyword");
    }
    let (name, members): (&RawIdentifierSyntax, usize) = match &raw.kind {
        RawDataDeclarationKind::Struct {
            interface_span,
            name,
            extends_span,
            marker_span,
            open_brace_span,
            fields,
            close_brace_span,
        } => {
            token(*interface_span, file, path, sources, errors, "interface", "interface keyword");
            token(*extends_span, file, path, sources, errors, "extends", "extends keyword");
            token(*marker_span, file, path, sources, errors, "ZrynaStruct", "struct marker");
            token(*open_brace_span, file, path, sources, errors, "{", "declaration open brace");
            token(*close_brace_span, file, path, sources, errors, "}", "declaration close brace");
            let mut member_names = BTreeSet::new();
            for field in fields {
                checked_span(field.span, file, path, sources, errors, "struct field");
                identifier(&field.name, file, path, sources, errors, "field name");
                if !member_names.insert(field.name.text.as_str()) {
                    errors.node(path, "duplicate struct field name");
                }
                token(field.colon_span, file, path, sources, errors, ":", "field colon");
                token(field.semicolon_span, file, path, sources, errors, ";", "field semicolon");
                own_type(field.type_syntax, type_owners, path, errors);
            }
            (name, fields.len())
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
            token(*interface_span, file, path, sources, errors, "interface", "interface keyword");
            token(*extends_span, file, path, sources, errors, "extends", "extends keyword");
            token(*marker_span, file, path, sources, errors, "ZrynaEnum", "enum marker");
            token(*open_brace_span, file, path, sources, errors, "{", "declaration open brace");
            token(*close_brace_span, file, path, sources, errors, "}", "declaration close brace");
            let mut member_names = BTreeSet::new();
            for variant in variants {
                checked_span(variant.span, file, path, sources, errors, "enum variant");
                identifier(&variant.name, file, path, sources, errors, "variant name");
                if !member_names.insert(variant.name.text.as_str()) {
                    errors.node(path, "duplicate enum variant name");
                }
                token(variant.colon_span, file, path, sources, errors, ":", "variant colon");
                token(
                    variant.semicolon_span,
                    file,
                    path,
                    sources,
                    errors,
                    ";",
                    "variant semicolon",
                );
                match (variant.payload_type, variant.none_span) {
                    (Some(root), None) => own_type(root, type_owners, path, errors),
                    (None, Some(span)) => {
                        token(
                            span,
                            file,
                            path,
                            sources,
                            errors,
                            "ZrynaNone",
                            "payload-free marker",
                        );
                    }
                    _ => errors.node(path, "enum variant must contain exactly one payload form"),
                }
            }
            (name, variants.len())
        }
    };
    let _ = members;
    identifier(name, file, path, sources, errors, "data declaration name");
    if !contains_claim(raw.span, name.span) {
        errors.node(path, "data declaration name is outside its owner span");
    }
    if !top_names.insert(name.text.clone()) {
        errors.node(path, "duplicate top-level declaration name");
    }
}
