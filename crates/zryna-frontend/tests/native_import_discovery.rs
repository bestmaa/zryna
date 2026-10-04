//! Native import discovery preserves source identity, spans and exact binding limits.

use zryna_frontend::{native_lexer, native_parser::v3::discover_import_candidates};
use zryna_source::{SourceFileInput, SourceMap};

fn map(text: &str) -> SourceMap {
    SourceMap::build(vec![SourceFileInput {
        path: "src/main.zry".to_owned(),
        text: text.to_owned(),
    }])
    .expect("original immutable source authority")
}

#[test]
fn import_discovery_preserves_original_utf8_crlf_spans_and_skips_complete_bodies() {
    let text = concat!(
        "// 😀 original bytes\r\n",
        "import { value as result, other, } from './dep.zry';\r\n",
        "interface Pair extends ZrynaStruct { value: i32; }\r\n",
        "export function main(): String { return \"import from './hidden.zry'\"; }\r\n",
    );
    let sources = map(text);
    let lexed = native_lexer::lex(&sources).expect("native lexical authority");
    let files =
        discover_import_candidates(&sources, &lexed).expect("untrusted discovery candidates");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].imports.len(), 1);
    assert_eq!(files[0].path, "src/main.zry");
    let import = &files[0].imports[0];
    assert_eq!(import.specifier.text, "./dep.zry");
    assert_eq!(import.bindings.len(), 2);
    assert_eq!(import.bindings[0].local.text, "result");
    let resolved = sources
        .resolve(sources.verify_span(import.specifier.value_span).expect("original span"))
        .expect("issuing map span");
    assert_eq!(
        &resolved.source().text()
            [import.specifier.value_span.start as usize..import.specifier.value_span.end as usize],
        "./dep.zry"
    );
    let source = sources.source(sources.verify_file_id(0).expect("id")).expect("source");
    assert_eq!(source.text(), text);
}

#[test]
fn import_discovery_rejects_foreign_lexical_identity_and_malformed_delimiters() {
    let sources = map("import { value } from './dep.zry';\n");
    let foreign = map("import { value } from './dep.zry';\n");
    let lexed = native_lexer::lex(&sources).expect("native tokens");
    assert!(discover_import_candidates(&foreign, &lexed).is_err());
    for text in [
        "import { value } from './dep.zry'",
        "import {} from './dep.zry';",
        "export function main(): i32 { return 1;",
        "export function main(]: i32 { return 1; }",
    ] {
        let sources = map(text);
        let lexed = native_lexer::lex(&sources).expect("native token stream");
        assert!(discover_import_candidates(&sources, &lexed).is_err(), "{text}");
    }
}

#[test]
fn import_discovery_preserves_exact_binding_limit_and_rejects_first_extra() {
    for extra in [0, 1] {
        let names = (0..zryna_frontend::syntax_v3::MAX_IMPORTED_NAMES_PER_DECLARATION + extra)
            .map(|index| format!("value{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let text = format!(
            "import {{ {names} }} from './dep.zry';\nexport function main(): i32 {{ return 1; }}"
        );
        let sources = map(&text);
        let lexed =
            native_lexer::lex(&sources).expect("native tokens within unchanged lexical budget");
        let result = discover_import_candidates(&sources, &lexed);
        if extra == 0 {
            assert_eq!(
                result.expect("exact named-binding limit")[0].imports[0].bindings.len(),
                zryna_frontend::syntax_v3::MAX_IMPORTED_NAMES_PER_DECLARATION
            );
        } else {
            assert_eq!(
                result.err().expect("first extra binding rejects").diagnostic().code(),
                "ZRYNA-F1002"
            );
        }
    }
}
