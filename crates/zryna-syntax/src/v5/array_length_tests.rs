//! Direct DTO callers must receive the same decimal grammar as wire callers.

use super::*;
use crate::v4::{RawBlockSyntax, RawIdentifierSyntax};
use zryna_source::{SourceFileInput, SourceMap, UntrustedSpan};

fn snapshot(spelling: &str, length: u32) -> (RawProjectSyntaxSnapshot, SourceMap) {
    let source = format!("function f():FixedArray<i32,{spelling}> {{}}");
    let span = |start: usize, end: usize| UntrustedSpan {
        file: 0,
        start: u32::try_from(start).expect("fixture start"),
        end: u32::try_from(end).expect("fixture end"),
    };
    let element = source.find("i32").expect("element");
    let length_start = element + 4;
    let greater = length_start + spelling.len();
    let open = greater + 2;
    let body_span = span(open, open + 2);
    let raw = RawProjectSyntaxSnapshot {
        schema_version: 5,
        diagnostics: vec![],
        files: vec![RawSourceUnit {
            id: 0,
            path: "array.zry".into(),
            imports: vec![],
            data_declarations: vec![],
            type_syntax: vec![
                RawTypeSyntax {
                    span: span(element, element + 3),
                    kind: RawTypeSyntaxKind::Named {
                        name: RawIdentifierSyntax {
                            text: "i32".into(),
                            span: span(element, element + 3),
                        },
                    },
                },
                RawTypeSyntax {
                    span: span(13, greater + 1),
                    kind: RawTypeSyntaxKind::FixedArray {
                        keyword_span: span(13, 23),
                        less_than_span: span(23, 24),
                        element: 0,
                        comma_span: span(element + 3, length_start),
                        length_span: span(length_start, greater),
                        length_spelling: spelling.into(),
                        length,
                        greater_than_span: span(greater, greater + 1),
                    },
                },
            ],
            functions: vec![RawFunctionSyntax {
                span: span(0, source.len()),
                export_span: None,
                function_span: span(0, 8),
                name: RawIdentifierSyntax { text: "f".into(), span: span(9, 10) },
                type_parameters: None,
                parameters: vec![],
                result_type: 1,
                body: RawFunctionBodySyntax {
                    span: body_span,
                    root_block: 0,
                    blocks: vec![RawBlockSyntax {
                        span: body_span,
                        open_brace_span: span(open, open + 1),
                        statements: vec![],
                        close_brace_span: span(open + 1, open + 2),
                    }],
                    statements: vec![],
                    expressions: vec![],
                },
            }],
        }],
    };
    let map = SourceMap::build(vec![SourceFileInput { path: "array.zry".into(), text: source }])
        .expect("independent source");
    (raw, map)
}

#[test]
fn direct_fixed_array_lengths_accept_canonical_zero_and_exact_limit() {
    for length in [0, 1, 10, 1_048_576] {
        let (raw, map) = snapshot(&length.to_string(), length);
        verify_snapshot(raw.clone(), &map).expect("canonical direct DTO");
        decode_snapshot(&serde_json::to_vec(&raw).expect("wire control"))
            .expect("same canonical wire grammar");
    }
}

#[test]
fn direct_fixed_array_lengths_reject_noncanonical_source_spellings() {
    for (spelling, length) in [
        ("+1", 1),
        ("01", 1),
        ("00", 0),
        ("-0", 0),
        ("-1", 1),
        ("1_0", 10),
        ("１", 1),
        ("١", 1),
        ("1.0", 1),
        ("0x1", 1),
        ("4294967296", 0),
        ("1048577", 1_048_577),
        ("1", 2),
        ("", 0),
        ("1\r\n", 1),
        ("1\u{2028}", 1),
        ("1\u{2029}", 1),
    ] {
        let (raw, map) = snapshot(spelling, length);
        let RawTypeSyntaxKind::FixedArray { length_span, .. } = raw.files[0].type_syntax[1].kind
        else {
            panic!("fixed array DTO")
        };
        let expected_span = map.verify_span(length_span).expect("original length span");
        let errors = verify_snapshot(raw, &map).expect_err(spelling);
        assert_eq!(errors[0].code, "ZRYNA-Y5001", "{spelling}");
        assert_eq!(errors[0].span, Some(expected_span), "{spelling}");
    }
}
