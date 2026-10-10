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
    let fixtures = copy_sources(&workspace, &fixture_root, 95);
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
    assert_eq!(accepted, 94);
    assert_eq!(hostile, 1);
}

#[cfg(feature = "native-provider-internal")]
#[test]
fn private_m2_bare_import_presentation_preserves_generic_parser_rejection() {
    use zryna_diagnostics::Diagnostic;

    for (index, declaration) in [
        "import \"./dep.zry\";",
        "// comment\r\nimport /* comment */ './dep.zry';",
        "import\n\"./dep.zry\";",
    ]
    .into_iter()
    .enumerate()
    {
        let workspace = Workspace::new(&format!("private-m2-bare-{index}"));
        workspace.write(
            "main.zry",
            format!("{declaration}\nexport function main(): i32 {{ return 7; }}\n"),
        );
        workspace.write("dep.zry", "export function value(): i32 { return 7; }\n");
        let root = workspace.root();
        let generic = capture_native_workspace_sources(&root, path("main.zry"))
            .err()
            .expect("generic capture still rejects unsupported named-import syntax");
        assert_eq!(super::code(&generic), "ZRYNA-F2002");
        assert!(generic.diagnostics()[0].primary_span().is_some());
        let m2 = crate::native_frontend::capture_control_flow_sources(&root, path("main.zry"))
            .err()
            .expect("private M2 must retain atomic rejection");
        assert_eq!(
            m2.diagnostics(),
            &[Diagnostic::error(
                "ZRYNA-F1103",
                None,
                "ZRYNA-F1103: frontend worker rejected a protocol request",
                "verify the pinned Node.js runtime and TypeScript frontend, then retry",
            )]
        );
    }
}

#[cfg(feature = "native-provider-internal")]
#[test]
fn private_m2_cycles_keep_canonical_path_and_generic_ownership_rejection() {
    use zryna_diagnostics::Diagnostic;

    for entry in ["dep.zry", "main.zry"] {
        let workspace = Workspace::new("private-m2-cycle");
        workspace.write(
            "main.zry",
            "import { value } from './dep.zry';\nexport function main(): i32 { return value(); }\n",
        );
        workspace.write(
            "dep.zry",
            "import { main } from './main.zry';\nexport function value(): i32 { return main(); }\n",
        );
        let root = workspace.root();
        let generic =
            capture_native_workspace_sources(&root, path(entry)).err().expect("generic cycle");
        assert_eq!(super::code(&generic), "ZRYNA-D3301");
        let m2 = crate::native_frontend::capture_control_flow_sources(&root, path(entry))
            .err()
            .expect("private M2 cycle");
        assert_eq!(
            m2.diagnostics(),
            &[Diagnostic::error(
                "ZRYNA-D3007",
                None,
                "dep.zry: module import graph contains a cycle",
                "remove self imports and cyclic dependency paths",
            )]
        );
        let canonical =
            discover_native_straight_line_closure(&root, path(entry)).expect_err("canonical cycle");
        assert_eq!(m2.diagnostics(), canonical.diagnostics());
    }
    let workspace = Workspace::new("private-m2-self-cycle");
    workspace.write(
        "self.zry",
        "import { main } from './self.zry';\nexport function main(): i32 { return 7; }\n",
    );
    let root = workspace.root();
    let m2 = crate::native_frontend::capture_control_flow_sources(&root, path("self.zry"))
        .err()
        .expect("private self cycle");
    let canonical = discover_native_straight_line_closure(&root, path("self.zry"))
        .expect_err("canonical self cycle");
    assert_eq!(m2.diagnostics(), canonical.diagnostics());
}

#[cfg(feature = "native-provider-internal")]
#[test]
fn private_m2_does_not_recode_other_discovery_errors() {
    for source in [
        "import value from './dep.zry';",
        "import { value from './dep.zry';",
        "function main(): i32 { import './dep.zry'; return 7; }",
        "import './dep.zry'; /* unterminated",
        "export function main(): i32 { return 'unterminated; }",
    ] {
        let workspace = Workspace::new("private-m2-other-errors");
        workspace.write("main.zry", source);
        let root = workspace.root();
        let generic = capture_native_workspace_sources(&root, path("main.zry"));
        let m2 = crate::native_frontend::capture_control_flow_sources(&root, path("main.zry"));
        match (generic, m2) {
            (Err(generic), Err(m2)) => {
                // Separate captures issue distinct nominal map identities. Stable serialization
                // retains every diagnostic field, canonical file index and exact byte span.
                assert_eq!(
                    serde_json::to_value(m2.diagnostics()).expect("private diagnostics"),
                    serde_json::to_value(generic.diagnostics()).expect("generic diagnostics")
                );
            }
            (Ok(generic), Ok(m2)) => {
                let generic = generic.verify_v3().err().expect("complete parser rejects");
                let m2 = m2.verify_v3().err().expect("complete parser rejects");
                assert_eq!(
                    serde_json::to_value(m2.diagnostics()).expect("private diagnostics"),
                    serde_json::to_value(generic.diagnostics()).expect("generic diagnostics")
                );
            }
            _ => panic!("private profile changed admission for {source}"),
        }
    }
}

#[cfg(feature = "native-provider-internal")]
#[test]
fn private_m2_retains_original_map_graph_and_live_source_guard() {
    let workspace = Workspace::new("private-m2-retained-owner");
    workspace.write(
        "main.zry",
        "import { value } from './dep.zry';\nexport function main(): i32 { return value(); }\n",
    );
    workspace.write("dep.zry", "export function value(): i32 { return 7; }\n");
    let root = workspace.root();
    let source = crate::native_frontend::capture_control_flow_sources(&root, path("main.zry"))
        .expect("private capture");
    let original = source.sources().clone();
    let graph = *source.graph_sha256_v3();
    let native = source.verify_v3().expect("mandatory verified syntax");
    assert!(native.closure().syntax().is_bound_to(&original));
    assert_eq!(native.closure().graph_sha256(), &graph);
    let canonical =
        discover_native_straight_line_closure(&root, path("main.zry")).expect("canonical closure");
    assert_eq!(native.closure().modules(), canonical.modules());
    assert_eq!(native.closure().graph_sha256(), canonical.graph_sha256());
    native.revalidate().expect("same original retained owner");
    // Unix permits the external write; the retained handle must detect it. Windows denies it.
    #[cfg(unix)]
    {
        workspace.write("dep.zry", "export function value(): i32 { return 9; }\n");
        let error = native.revalidate().expect_err("retained source mutation must reject");
        assert_eq!(super::code(&error), "ZRYNA-D3004");
    }
}
