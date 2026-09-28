//! Native v4 syntax coverage for the checked-in M3 source corpus.

use std::{fs, path::Path};

use zryna_frontend::{native_lexer::lex, native_parser::v4::parse_v4_candidate, syntax_v4};
use zryna_source::{SourceFileInput, SourceMap};

#[test]
fn checked_in_m3_sources_form_verifiable_native_candidates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/m3-fixtures");
    let mut failures = Vec::new();
    let mut checked = 0;
    let mut verifier_hostile = 0;
    for entry in fs::read_dir(root).expect("M3 fixture directory") {
        let entry = entry.expect("fixture entry");
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "zry") {
            continue;
        }
        let name = path.file_name().expect("fixture name").to_string_lossy();
        let text = fs::read_to_string(&path).expect("UTF-8 fixture");
        let sources = SourceMap::build(vec![SourceFileInput { path: format!("src/{name}"), text }])
            .expect("source map");
        let lexed = lex(&sources).expect("bounded lexical stream");
        match parse_v4_candidate(&sources, &lexed) {
            Ok(raw) => {
                if let Err(error) = syntax_v4::verify_snapshot(raw, &sources) {
                    if name == "borrow-exclusive-nonreference.zry"
                        && error.iter().any(|diagnostic| diagnostic.code() == "ZRYNA-Y4002")
                    {
                        verifier_hostile += 1;
                    } else {
                        failures.push(format!("{name}: verifier: {error:?}"));
                    }
                }
            }
            Err(error) => failures.push(format!("{name}: {}", error.diagnostic())),
        }
        checked += 1;
    }
    assert!(checked > 50, "expected the M3 source corpus");
    assert_eq!(verifier_hostile, 1, "expected the known invalid borrow operand");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
