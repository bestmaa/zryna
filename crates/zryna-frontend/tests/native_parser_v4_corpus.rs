//! Native v4 syntax coverage for the checked-in M3 source corpus.

use zryna_frontend::{native_lexer::lex, native_parser::v4::parse_v4_candidate, syntax_v4};
use zryna_source::{SourceFileInput, SourceMap};

#[path = "native_parser_v4_corpus/fixtures.rs"]
mod fixtures;

#[test]
fn checked_in_m3_sources_form_verifiable_native_candidates() {
    let mut failures = Vec::new();
    let mut checked = 0;
    let mut verifier_hostile = 0;
    for (path, text) in fixtures::sources() {
        let sources = SourceMap::build(vec![SourceFileInput { path: path.clone(), text }])
            .expect("source map");
        let lexed = lex(&sources).expect("bounded lexical stream");
        match parse_v4_candidate(&sources, &lexed) {
            Ok(raw) => {
                if let Err(error) = syntax_v4::verify_snapshot(raw, &sources) {
                    if path == "src/borrow-exclusive-nonreference.zry" {
                        assert_eq!(error.len(), 1, "known hostile source diagnostic count");
                        assert_eq!(
                            error[0].code(),
                            "ZRYNA-Y4002",
                            "known hostile source verifier diagnostic"
                        );
                        verifier_hostile += 1;
                    } else {
                        failures.push(format!("{path}: verifier: {error:?}"));
                    }
                }
            }
            Err(error) => failures.push(format!("{path}: {}", error.diagnostic())),
        }
        checked += 1;
    }
    assert_eq!(checked, 97, "complete M3 source corpus");
    assert_eq!(verifier_hostile, 1, "expected the known invalid borrow operand");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
