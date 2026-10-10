use std::{fs, path::Path, time::Duration};

use sha2::{Digest as _, Sha256};
use zryna_frontend::{
    VerifiedFrontendProviderV4, WorkerError, native_lexer, native_parser, syntax_v4,
};
use zryna_source::SourceMap;

use super::{Workspace, path};
use crate::{
    capture_native_workspace_sources, discover_native_straight_line_closure,
    discover_ownership_module_closure,
};

fn copy_sources(workspace: &Workspace, fixture_root: &Path, expected: usize) -> Vec<String> {
    let mut directories = vec![fixture_root.to_owned()];
    let mut sources = Vec::new();
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).expect("complete fixture directory") {
            let filename = entry.expect("fixture entry").path();
            if filename.is_dir() {
                directories.push(filename);
            } else if filename.extension().is_some_and(|extension| extension == "zry") {
                let relative = filename
                    .strip_prefix(fixture_root)
                    .expect("fixture relative path")
                    .components()
                    .map(|part| part.as_os_str().to_str().expect("ASCII fixture path"))
                    .collect::<Vec<_>>()
                    .join("/");
                let source_path = format!("src/{relative}");
                workspace.write(&source_path, fs::read(filename).expect("exact fixture bytes"));
                sources.push(source_path);
            }
        }
    }
    sources.sort();
    assert_eq!(sources.len(), expected, "complete frozen fixture inventory");
    sources
}

#[test]
fn native_resolution_matches_canonical_graphs_for_every_complete_m2_fixture() {
    let workspace = Workspace::new("complete-m2-corpus");
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/m2-fixtures");
    let fixtures = copy_sources(&workspace, &fixture_root, 14);
    let root = workspace.root();
    let mut accepted = 0;
    let mut rejected = 0;
    for fixture in fixtures {
        let canonical = discover_native_straight_line_closure(&root, path(&fixture));
        let native = capture_native_workspace_sources(&root, path(&fixture))
            .and_then(super::super::NativeSourceSnapshot::verify_v3);
        match (canonical, native) {
            (Ok(canonical), Ok(native)) => {
                assert_eq!(native.closure().modules(), canonical.modules(), "{fixture}");
                assert_eq!(native.closure().graph_sha256(), canonical.graph_sha256(), "{fixture}");
                assert_eq!(native.closure().edges().len(), canonical.edges().len(), "{fixture}");
                native.revalidate().expect("unchanged admitted source through final comparison");
                accepted += 1;
            }
            (Err(_), Err(_)) => rejected += 1,
            _ => panic!("native and canonical source closures disagree for {fixture}"),
        }
    }
    assert!(accepted > 0 && rejected > 0, "both real admitted and hostile fixture paths execute");
    assert_eq!(accepted + rejected, 14);
}

struct NativeCandidate;

impl VerifiedFrontendProviderV4 for NativeCandidate {
    fn minimum_analysis_timeout(&self) -> Duration {
        Duration::ZERO
    }

    fn analyze_verified_v4(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v4::ProjectSyntaxSnapshot, WorkerError> {
        // Genuine complete syntax only: this corpus excludes its one known verifier-hostile source.
        let lexed = native_lexer::lex(sources).expect("admitted complete lexical fixture");
        let raw =
            native_parser::v4::parse_v4_candidate(sources, &lexed).expect("genuine full candidate");
        Ok(syntax_v4::verify_snapshot(raw, sources).expect("existing mandatory syntax verifier"))
    }
}

#[test]
fn native_resolution_matches_all_admitted_m3_fixture_graphs_and_preserves_hostile_rejection() {
    let workspace = Workspace::new("complete-m3-corpus");
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/m3-fixtures");
    let fixtures = copy_sources(&workspace, &fixture_root, 97);
    let registry: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/m3-conformance-v1.json"
    )))
    .expect("frozen conformance registry");
    let registered = registry["fixtures"].as_array().expect("registered sources");
    let root = workspace.root();
    let mut accepted = 0;
    let mut hostile = 0;
    for fixture in fixtures {
        let registered_path =
            format!("tests/m3-fixtures/{}", fixture.strip_prefix("src/").expect("corpus path"));
        if let Some(dependency) = registered
            .iter()
            .find(|source| source["path"] == registered_path)
            .and_then(|source| source["dependency"].as_str())
        {
            let source = registered
                .iter()
                .find(|source| source["id"] == dependency)
                .expect("registered dependency");
            let filename = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(source["path"].as_str().expect("dependency path"));
            let bytes = fs::read(filename).expect("original dependency bytes");
            assert_eq!(format!("{:x}", Sha256::digest(&bytes)), source["sha256"]);
            workspace.write("src/conformance/math.zry", bytes);
        }
        let native = capture_native_workspace_sources(&root, path(&fixture))
            .and_then(super::super::NativeSourceSnapshot::verify_v4);
        if fixture == "src/borrow-exclusive-nonreference.zry" {
            assert_eq!(
                super::code(&native.err().expect("known independently hostile source")),
                "ZRYNA-Y4002"
            );
            hostile += 1;
            continue;
        }
        let native = native.unwrap_or_else(|error| panic!("{fixture}: {error:?}"));
        let canonical = discover_ownership_module_closure(&root, path(&fixture), &NativeCandidate)
            .unwrap_or_else(|error| panic!("{fixture}: canonical {error:?}"));
        assert_eq!(native.closure().modules(), canonical.modules(), "{fixture}");
        assert_eq!(native.closure().graph_sha256(), canonical.graph_sha256(), "{fixture}");
        assert_eq!(native.closure().edges().len(), canonical.edges().len(), "{fixture}");
        native.revalidate().expect("original source closure retained throughout corpus comparison");
        accepted += 1;
    }
    assert_eq!(accepted, 96);
    assert_eq!(hostile, 1);
}
