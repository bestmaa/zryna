//! Frozen controls from the pinned scanner and TypeScript parse diagnostics.

use serde_json::{Value, json};
use zryna_frontend::native_lexer::{TokenKind, lex};
use zryna_source::{SourceFileInput, SourceMap};

#[test]
fn unicode_gaps_and_line_comments_match_pinned_scanner_controls() {
    let corpus: Value = serde_json::from_str(include_str!("native_lexer_whitespace.json"))
        .expect("frozen scanner controls");
    assert_eq!(corpus["provider_version"], "6.0.3");
    let cases = corpus["cases"].as_array().expect("scanner cases");
    assert_eq!(cases.len(), 34, "complete whitespace and rejected-gap controls");
    for case in cases {
        let text = case["text"].as_str().expect("source text");
        let sources = SourceMap::build(vec![SourceFileInput {
            path: "src/main.zry".to_owned(),
            text: text.to_owned(),
        }])
        .expect("bounded source");
        let lexed = lex(&sources).expect("bounded scanner control");
        let tokens = lexed.files()[0]
            .tokens()
            .map(|token| {
                let span = token.span();
                json!({"start":span.start(),"end":span.end(),
                "text":&text[span.start() as usize..span.end() as usize]})
            })
            .collect::<Vec<_>>();
        assert_eq!(
            json!(tokens),
            case["tokens"],
            "{}: exact token spans and spelling",
            case["name"]
        );
        if case["accepted"] == true {
            assert_eq!(case["parse_codes"], json!([]));
            assert!(lexed.diagnostics().is_empty(), "{}: admitted gap", case["name"]);
        } else {
            assert_eq!(case["parse_codes"], json!([1128, 1127]));
            assert_eq!(lexed.diagnostics().len(), 1, "one malformed source byte range");
            assert_eq!(lexed.diagnostics()[0].code(), "ZRYNA-F1501");
            let invalid = lexed.files()[0]
                .tokens()
                .find(|token| token.kind() == TokenKind::Invalid)
                .expect("rejected gap token");
            assert_eq!(lexed.diagnostics()[0].primary_span(), Some(invalid.span()));
        }
    }
}
