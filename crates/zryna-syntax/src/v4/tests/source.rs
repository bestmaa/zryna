use super::*;
use crate::v4::verify::valid_identifier;

#[test]
fn version_and_source_claims_fail_closed() {
    let authority = sources(SOURCE);
    let mut wrong_version = raw();
    wrong_version.schema_version = 3;
    assert!(
        verify_snapshot(wrong_version, &authority)
            .unwrap_err()
            .iter()
            .any(|d| d.to_string().contains("ZRYNA-Y4001"))
    );
    let mut wrong_span = raw();
    wrong_span.files[0].functions[0].function_span = uspan(SOURCE, "one");
    assert!(
        verify_snapshot(wrong_span, &authority)
            .unwrap_err()
            .iter()
            .any(|d| d.to_string().contains("ZRYNA-Y4002"))
    );
}

#[test]
fn source_structure_rejects_fabricated_call_and_type_claims() {
    let authority = sources(SOURCE);
    let mut call = raw();
    let function_name = call.files[0].functions[0].name.clone();
    let open = uspan(SOURCE, "(");
    let close = uspan(SOURCE, ")");
    call.files[0].functions[0].body.expressions[0].kind = RawExpressionKind::Call {
        callee: function_name,
        open_paren_span: open,
        arguments: vec![],
        close_paren_span: close,
    };
    assert!(verify_snapshot(call, &authority).is_err());

    let mut ty = raw();
    ty.files[0].type_syntax[0].kind =
        RawTypeSyntaxKind::Named { name: ty.files[0].functions[0].name.clone() };
    assert!(verify_snapshot(ty, &authority).is_err());
}

#[test]
fn single_quoted_import_specifier_is_source_faithful() {
    const IMPORTED: &str = "import { dep } from './dep.zry';\nfunction one(): i32 { return 1; }";
    let import_end = IMPORTED.find(';').unwrap() + 1;
    let imported = IMPORTED.find("dep").unwrap();
    let from = IMPORTED.find("from").unwrap();
    let token_start = IMPORTED.find("'./dep.zry'").unwrap();
    let value_start = token_start + 1;
    let binding =
        RawIdentifierSyntax { text: "dep".into(), span: span_range(imported, imported + 3) };
    let snapshot = RawProjectSyntaxSnapshot {
        schema_version: 4,
        diagnostics: vec![],
        files: vec![RawSourceUnit {
            id: 0,
            path: "src/main.zry".into(),
            imports: vec![RawImportSyntax {
                span: span_range(0, import_end),
                import_span: span_range(0, 6),
                bindings: vec![RawImportBindingSyntax {
                    span: binding.span,
                    imported: binding.clone(),
                    local: binding,
                    as_span: None,
                }],
                from_span: span_range(from, from + 4),
                specifier: RawModuleSpecifierSyntax {
                    text: "./dep.zry".into(),
                    token_span: span_range(token_start, token_start + 11),
                    value_span: span_range(value_start, value_start + 9),
                },
                semicolon_span: span_range(import_end - 1, import_end),
            }],
            type_syntax: vec![named_type(IMPORTED, 0)],
            data_declarations: vec![],
            functions: vec![function_for(IMPORTED, 0)],
        }],
    };
    assert!(verify_snapshot(snapshot, &sources(IMPORTED)).is_ok());
}

#[test]
fn identifiers_use_the_portable_ascii_profile() {
    assert!(valid_identifier("_valid123"));
    assert!(!valid_identifier("9invalid"));
    assert!(!valid_identifier("with-dash"));
    assert!(!valid_identifier("café"));
    assert!(!valid_identifier(&"a".repeat(129)));
}
