//! Direct comparison between the native lexer and the exact pinned protocol-v4 provider.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use zryna_frontend::native_lexer::{Keyword, Token, TokenKind, lex};
use zryna_source::{SourceFileInput, SourceMap};

const POSITIVE: &str = include_str!("../../../tests/provider-conformance-v4/fixtures/positive.zry");
const ORDERING_A: &str =
    include_str!("../../../tests/provider-conformance-v4/fixtures/ordering-a.zry");
const ORDERING_Z: &str =
    include_str!("../../../tests/provider-conformance-v4/fixtures/ordering-z.zry");
const NEGATIVE: &str =
    include_str!("../../../tests/provider-conformance-v4/fixtures/lexical-negative.zry");

#[test]
#[ignore = "requires the exact pinned TypeScript provider"]
fn pinned_provider_and_native_lexer_match_exact_lexical_boundaries() {
    compare_positive(&[("src/main.zry", POSITIVE)]);
    compare_positive(&[("src/z.zry", ORDERING_Z), ("src/a.zry", ORDERING_A)]);
    compare_negative();
    for character in [
        '\u{0085}', '\u{00a0}', '\u{1680}', '\u{2000}', '\u{200b}', '\u{202f}', '\u{205f}',
        '\u{3000}', '\u{feff}',
    ] {
        let text = format!("export{character}function f(x: i32,): i32 {{return x;}}");
        compare_positive(&[("src/main.zry", &text)]);
    }
    for character in ['\r', '\n', '\u{2028}', '\u{2029}'] {
        let text = format!("// comment{character}export function f(): i32 {{return 1;}}");
        compare_positive(&[("src/main.zry", &text)]);
    }
}

fn compare_positive(inputs: &[(&str, &str)]) {
    let first = provider_witness(inputs);
    assert_eq!(first, provider_witness(inputs), "complete provider witness replay");
    assert_eq!(first["provider"], json!({ "id": "typescript-6", "version": "6.0.3" }));
    assert_eq!(first["analysis"]["kind"], "snapshot");
    assert_eq!(first["analysis"]["diagnostic_codes"], json!([]));

    let sources = source_map(inputs);
    let native = lex(&sources).expect("provider-positive source must lex");
    assert!(native.is_bound_to(&sources));
    let order = native
        .files()
        .iter()
        .map(|file| json!({ "id": file.id().index(), "path": file.path().as_str() }))
        .collect::<Vec<_>>();
    assert_eq!(first["analysis"]["file_order"], json!(order));
    compare_files(&first, &sources, &native);
}

fn compare_negative() {
    let inputs = [("src/negative.zry", NEGATIVE)];
    let first = provider_witness(&inputs);
    assert_eq!(first, provider_witness(&inputs), "negative provider witness replay");
    assert_eq!(first["files"][0]["tokens"][0]["raw_provider_kind"], "QuestionToken");
    assert_eq!(first["files"][0]["tokens"][0]["canonical_kind"], "invalid");
    assert_eq!(first["analysis"]["kind"], "diagnostic");
    assert_eq!(first["analysis"]["code"], "ZRYNA-F2002");

    let sources = source_map(&inputs);
    let native = lex(&sources).expect("unsupported token must recover lexically");
    assert!(native.is_bound_to(&sources));
    compare_files(&first, &sources, &native);
    assert_eq!(native.diagnostics().len(), 1);
    let diagnostic = &native.diagnostics()[0];
    assert_eq!(diagnostic.code(), "ZRYNA-F1501");
    let span = diagnostic.primary_span().expect("native lexical diagnostic span");
    assert_eq!(
        json!({ "file": span.file().index(), "start": span.start(), "end": span.end() }),
        json!({
            "file": first["analysis"]["file"],
            "start": first["analysis"]["start"],
            "end": first["analysis"]["end"],
        })
    );
}

fn compare_files(
    witness: &Value,
    sources: &SourceMap,
    native: &zryna_frontend::native_lexer::LexedProject,
) {
    let provider_files = witness["files"].as_array().expect("provider files");
    assert_eq!(provider_files.len(), native.files().len());
    for (provider, file) in provider_files.iter().zip(native.files()) {
        assert_eq!(provider["path"], file.path().as_str());
        let source = sources.source(file.id()).expect("native file source authority");
        assert_eq!(provider["text"], source.text());
        let tokens =
            file.tokens().map(|token| native_token(source.text(), token)).collect::<Vec<_>>();
        let expected = provider["tokens"]
            .as_array()
            .expect("provider tokens")
            .iter()
            .map(|token| {
                json!({
                    "kind": token["canonical_kind"],
                    "start": token["start"],
                    "end": token["end"],
                    "text": token["text"],
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(tokens, expected, "provider/native token identity for {}", file.path());
    }
}

fn native_token(source: &str, token: Token) -> Value {
    let span = token.span();
    json!({
        "kind": canonical_kind(token.kind()),
        "start": span.start(),
        "end": span.end(),
        "text": &source[span.start() as usize..span.end() as usize],
    })
}

fn canonical_kind(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Identifier => "identifier",
        TokenKind::Keyword(keyword) => match keyword {
            Keyword::As => "keyword-as",
            Keyword::Const => "keyword-const",
            Keyword::Else => "keyword-else",
            Keyword::Export => "keyword-export",
            Keyword::Extends => "keyword-extends",
            Keyword::False => "keyword-false",
            Keyword::From => "keyword-from",
            Keyword::Function => "keyword-function",
            Keyword::If => "keyword-if",
            Keyword::Import => "keyword-import",
            Keyword::Interface => "keyword-interface",
            Keyword::Let => "keyword-let",
            Keyword::Return => "keyword-return",
            Keyword::True => "keyword-true",
            Keyword::While => "keyword-while",
        },
        TokenKind::DecimalInteger => "decimal-integer",
        TokenKind::StringLiteral => "string-literal",
        TokenKind::OpenBrace => "open-brace",
        TokenKind::CloseBrace => "close-brace",
        TokenKind::OpenBracket => "open-bracket",
        TokenKind::CloseBracket => "close-bracket",
        TokenKind::OpenParen => "open-paren",
        TokenKind::CloseParen => "close-paren",
        TokenKind::Colon => "colon",
        TokenKind::Semicolon => "semicolon",
        TokenKind::Comma => "comma",
        TokenKind::Dot => "dot",
        TokenKind::LessThan => "less-than",
        TokenKind::LessEqual => "less-equal",
        TokenKind::GreaterThan => "greater-than",
        TokenKind::GreaterEqual => "greater-equal",
        TokenKind::Equals => "equals",
        TokenKind::FatArrow => "fat-arrow",
        TokenKind::StrictEqual => "strict-equal",
        TokenKind::StrictNotEqual => "strict-not-equal",
        TokenKind::Plus => "plus",
        TokenKind::Minus => "minus",
        TokenKind::Asterisk => "asterisk",
        TokenKind::Invalid => "invalid",
    }
}

fn source_map(inputs: &[(&str, &str)]) -> SourceMap {
    SourceMap::build(
        inputs
            .iter()
            .map(|(path, text)| SourceFileInput {
                path: (*path).to_owned(),
                text: (*text).to_owned(),
            })
            .collect(),
    )
    .expect("provider corpus source map")
}

fn provider_witness(inputs: &[(&str, &str)]) -> Value {
    let root = workspace_root();
    let mut child = Command::new("node")
        .arg(root.join("scripts/native-lexer-provider-witness.mjs"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pinned provider witness");
    let request = json!({
        "files": inputs.iter().map(|(path, text)| json!({ "path": path, "text": text })).collect::<Vec<_>>(),
    });
    child
        .stdin
        .take()
        .expect("provider witness stdin")
        .write_all(request.to_string().as_bytes())
        .expect("write provider witness request");
    let output = child.wait_with_output().expect("wait for provider witness");
    assert!(
        output.status.success(),
        "provider witness failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "provider witness stderr");
    serde_json::from_slice(&output.stdout).expect("closed provider witness JSON")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_owned()
}
