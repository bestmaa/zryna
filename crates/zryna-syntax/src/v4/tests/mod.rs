use super::*;
use zryna_source::SourceFileInput;

pub(super) const SOURCE: &str = "function one(): i32 { return 1; }";
pub(super) fn uspan(source: &str, needle: &str) -> UntrustedSpan {
    let start = source.find(needle).unwrap();
    UntrustedSpan {
        file: 0,
        start: u32::try_from(start).unwrap(),
        end: u32::try_from(start + needle.len()).unwrap(),
    }
}
pub(super) fn span_range(start: usize, end: usize) -> UntrustedSpan {
    UntrustedSpan {
        file: 0,
        start: u32::try_from(start).unwrap(),
        end: u32::try_from(end).unwrap(),
    }
}
pub(super) fn sources(text: &str) -> SourceMap {
    SourceMap::build(vec![SourceFileInput { path: "src/main.zry".into(), text: text.into() }])
        .unwrap()
}
pub(super) fn raw() -> RawProjectSyntaxSnapshot {
    let function = SOURCE.find("function").unwrap();
    let name = SOURCE.find("one").unwrap();
    let ty = SOURCE.find("i32").unwrap();
    let open = SOURCE.find('{').unwrap();
    let ret = SOURCE.find("return").unwrap();
    let literal = SOURCE.find('1').unwrap();
    let semi = SOURCE.find(';').unwrap();
    let close = SOURCE.rfind('}').unwrap();
    RawProjectSyntaxSnapshot {
        schema_version: 4,
        diagnostics: vec![],
        files: vec![RawSourceUnit {
            id: 0,
            path: "src/main.zry".into(),
            imports: vec![],
            type_syntax: vec![RawTypeSyntax {
                span: span_range(ty, ty + 3),
                kind: RawTypeSyntaxKind::Named {
                    name: RawIdentifierSyntax { text: "i32".into(), span: span_range(ty, ty + 3) },
                },
            }],
            data_declarations: vec![],
            functions: vec![RawFunctionSyntax {
                span: span_range(0, SOURCE.len()),
                export_span: None,
                function_span: span_range(function, function + 8),
                name: RawIdentifierSyntax { text: "one".into(), span: span_range(name, name + 3) },
                parameters: vec![],
                result_type: 0,
                body: RawFunctionBodySyntax {
                    span: span_range(open, close + 1),
                    root_block: 0,
                    blocks: vec![RawBlockSyntax {
                        span: span_range(open, close + 1),
                        open_brace_span: span_range(open, open + 1),
                        statements: vec![0],
                        close_brace_span: span_range(close, close + 1),
                    }],
                    statements: vec![RawStatementSyntax {
                        span: span_range(ret, semi + 1),
                        kind: RawStatementKind::Return {
                            keyword_span: span_range(ret, ret + 6),
                            value: 0,
                            semicolon_span: span_range(semi, semi + 1),
                        },
                    }],
                    expressions: vec![RawExpressionSyntax {
                        span: span_range(literal, literal + 1),
                        kind: RawExpressionKind::I32Literal { spelling: "1".into() },
                    }],
                },
            }],
        }],
    }
}

pub(super) fn nth(source: &str, needle: &str, occurrence: usize) -> usize {
    source.match_indices(needle).nth(occurrence).unwrap().0
}
pub(super) fn named_type(source: &str, occurrence: usize) -> RawTypeSyntax {
    let start = nth(source, "i32", occurrence);
    RawTypeSyntax {
        span: span_range(start, start + 3),
        kind: RawTypeSyntaxKind::Named {
            name: RawIdentifierSyntax { text: "i32".into(), span: span_range(start, start + 3) },
        },
    }
}
pub(super) fn struct_decl(
    source: &str,
    name: &str,
    field: &str,
    occurrence: usize,
    type_id: u32,
) -> RawDataDeclaration {
    let interface = nth(source, "interface", occurrence);
    let name_start = source[interface..].find(name).unwrap() + interface;
    let extends = source[interface..].find("extends").unwrap() + interface;
    let marker = source[interface..].find("ZrynaStruct").unwrap() + interface;
    let open = source[interface..].find('{').unwrap() + interface;
    let close = source[open..].find('}').unwrap() + open;
    let field_start = source[open..].find(field).unwrap() + open;
    let colon = source[field_start..].find(':').unwrap() + field_start;
    let semi = source[colon..].find(';').unwrap() + colon;
    RawDataDeclaration {
        span: span_range(interface, close + 1),
        export_span: None,
        kind: RawDataDeclarationKind::Struct {
            interface_span: span_range(interface, interface + 9),
            name: RawIdentifierSyntax {
                text: name.into(),
                span: span_range(name_start, name_start + name.len()),
            },
            extends_span: span_range(extends, extends + 7),
            marker_span: span_range(marker, marker + 11),
            open_brace_span: span_range(open, open + 1),
            fields: vec![RawDataField {
                span: span_range(field_start, semi + 1),
                name: RawIdentifierSyntax {
                    text: field.into(),
                    span: span_range(field_start, field_start + field.len()),
                },
                colon_span: span_range(colon, colon + 1),
                type_syntax: type_id,
                semicolon_span: span_range(semi, semi + 1),
            }],
            close_brace_span: span_range(close, close + 1),
        },
    }
}
pub(super) fn function_for(source: &str, type_id: u32) -> RawFunctionSyntax {
    let function = source.find("function").unwrap();
    let name = source[function..].find("one").unwrap() + function;
    let open = source[function..].find('{').unwrap() + function;
    let close = source[open..].find('}').unwrap() + open;
    let ret = source[open..].find("return").unwrap() + open;
    let literal = source[ret..].find('1').unwrap() + ret;
    let semi = source[ret..].find(';').unwrap() + ret;
    RawFunctionSyntax {
        span: span_range(function, close + 1),
        export_span: None,
        function_span: span_range(function, function + 8),
        name: RawIdentifierSyntax { text: "one".into(), span: span_range(name, name + 3) },
        parameters: vec![],
        result_type: type_id,
        body: RawFunctionBodySyntax {
            span: span_range(open, close + 1),
            root_block: 0,
            blocks: vec![RawBlockSyntax {
                span: span_range(open, close + 1),
                open_brace_span: span_range(open, open + 1),
                statements: vec![0],
                close_brace_span: span_range(close, close + 1),
            }],
            statements: vec![RawStatementSyntax {
                span: span_range(ret, semi + 1),
                kind: RawStatementKind::Return {
                    keyword_span: span_range(ret, ret + 6),
                    value: 0,
                    semicolon_span: span_range(semi, semi + 1),
                },
            }],
            expressions: vec![RawExpressionSyntax {
                span: span_range(literal, literal + 1),
                kind: RawExpressionKind::I32Literal { spelling: "1".into() },
            }],
        },
    }
}

mod arenas;
mod diagnostics;
mod resources;
mod source;
mod wire;
