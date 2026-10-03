use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use sha2::{Digest as _, Sha256};
use zryna_frontend::{native_lexer, native_parser, syntax_v3, syntax_v4};
use zryna_source::NormalizedSourcePath;

use super::{ModuleClosureError, capture_native_workspace_sources, graph};
use crate::{WorkspaceSourceRoot, discover_native_straight_line_closure};

mod corpus;
mod hostile;
mod package;
mod resources;

pub(super) struct Workspace(pub(super) PathBuf);

impl Workspace {
    pub(super) fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "zryna-native-source-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("unique source fixture");
        Self(path)
    }

    pub(super) fn write(&self, name: &str, source: impl AsRef<[u8]>) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().expect("source parent")).expect("source directory");
        fs::write(path, source).expect("complete source bytes");
    }

    pub(super) fn root(&self) -> WorkspaceSourceRoot {
        WorkspaceSourceRoot::capture(&self.0).expect("retained no-follow fixture root")
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("owned fixture cleanup");
    }
}

pub(super) fn path(value: &str) -> NormalizedSourcePath {
    NormalizedSourcePath::new(value).expect("portable fixture path")
}

pub(super) fn code(error: &ModuleClosureError) -> &str {
    error.diagnostics().first().expect("stable rejection diagnostic").code()
}

#[test]
fn native_snapshot_matches_canonical_driver_graph_and_original_worker_fixture() {
    let main = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../zryna-frontend/tests/native_parser_v3_calls/main.zry"
    ));
    let math = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../zryna-frontend/tests/native_parser_v3_calls/math.zry"
    ));
    let receipt = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../zryna-frontend/tests/native_parser_v3_calls/calls.snapshot.json"
    ));
    let workspace = Workspace::new("canonical");
    // The independent worker receipt was captured with CRLF source offsets.
    let main = main.replace("\r\n", "\n").replace('\n', "\r\n");
    workspace.write("src/main.zry", main);
    workspace.write("src/math.zry", math);
    let root = workspace.root();
    let source = capture_native_workspace_sources(&root, path("src/main.zry")).expect("capture");
    let lexed = native_lexer::lex(source.sources()).expect("original tokens");
    let raw = native_parser::v3::parse_v3_straight_line_candidate(source.sources(), &lexed)
        .expect("complete native candidate");
    assert_eq!(
        raw,
        syntax_v3::decode_snapshot(receipt).expect("independent pinned-worker receipt")
    );
    let original_map = source.sources().clone();
    let preparse_identity = *source.graph_sha256_v3();
    let native = source.verify_v3().expect("native verification");
    let canonical = discover_native_straight_line_closure(&root, path("src/main.zry"))
        .expect("existing canonical driver closure");
    assert_eq!(native.closure().modules(), canonical.modules());
    assert_eq!(native.closure().graph_sha256(), canonical.graph_sha256());
    assert_eq!(&preparse_identity, canonical.graph_sha256());
    assert!(native.closure().syntax().is_bound_to(&original_map));
    for edge in native.closure().edges() {
        assert!(original_map.resolve(edge.declaration_span()).is_ok());
        assert_eq!(edge.target().as_str(), "src/math.zry");
    }
    native.revalidate().expect("source unchanged through verification");
}

#[test]
fn native_v4_preserves_complete_data_fixture_and_original_source_identity() {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../zryna-frontend/tests/native_parser_v4_data/data.zry"
    ));
    let receipt = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../zryna-frontend/tests/native_parser_v4_data/data.snapshot.json"
    ));
    let workspace = Workspace::new("v4-original");
    workspace.write("src/data.zry", text);
    let root = workspace.root();
    let source = capture_native_workspace_sources(&root, path("src/data.zry")).expect("capture");
    let original_map = source.sources().clone();
    let expected =
        syntax_v4::decode_snapshot(receipt).expect("independent complete worker receipt");
    let lexed = native_lexer::lex(source.sources()).expect("tokens");
    assert_eq!(
        native_parser::v4::parse_v4_candidate(source.sources(), &lexed)
            .expect("complete candidate"),
        expected
    );
    let hash = *source.graph_sha256_v4();
    let native = source.verify_v4().expect("complete v4 verification");
    assert!(native.closure().syntax().is_bound_to(&original_map));
    assert_eq!(native.closure().graph_sha256(), &hash);
    assert_eq!(
        native
            .closure()
            .sources()
            .source(original_map.file_id(&path("src/data.zry")).expect("original file id"))
            .expect("retained source")
            .text(),
        text
    );
    native.revalidate().expect("source retained through verification");
}

#[test]
fn native_discovery_retains_crlf_utf8_and_dispatches_one_genuine_m2_program() {
    let workspace = Workspace::new("dispatch");
    let main = "// 😀 unchanged\r\nimport { value } from './dep.zry';\r\nexport function main(): i32 { return value(); }\r\n";
    let dep = "export function value(): i32 { return 7; }\r\n";
    workspace.write("main.zry", main);
    workspace.write("dep.zry", dep);
    let root = workspace.root();
    let native = capture_native_workspace_sources(&root, path("main.zry"))
        .expect("capture")
        .verify_v3()
        .expect("real v3 closure");
    let original_hash: [u8; 32] = Sha256::digest(main.as_bytes()).into();
    assert_eq!(native.closure().modules()[1].source_sha256(), &original_hash);
    let program =
        native.closure().lower_control_flow_v1().expect("mandatory semantic and IR verification");
    native.revalidate().expect("retained source before target dispatch");
    let javascript =
        zryna_backend_javascript::emit_control_flow(&program).expect("real JS emission");
    let wasm = zryna_backend_webassembly::emit_control_flow(&program).expect("real Wasm emission");
    assert!(!javascript.source.is_empty());
    assert!(!wasm.bytes().is_empty());
    native.revalidate().expect("same source after both target dispatches");
    let id = native.closure().sources().file_id(&path("main.zry")).expect("entry id");
    assert_eq!(native.closure().sources().source(id).expect("entry").text(), main);
}

#[test]
fn native_v2_recovery_keeps_original_source_bound_diagnostics() {
    let workspace = Workspace::new("recovery");
    workspace.write(
        "main.zry",
        "export function bad(): i32 { return (1); }\nexport function good(): i32 { return 7; }\n",
    );
    let root = workspace.root();
    let snapshot = capture_native_workspace_sources(&root, path("main.zry"))
        .expect("capture")
        .verify_v2()
        .expect("versioned recovery verification");
    assert!(snapshot.syntax().is_bound_to(snapshot.sources()));
    assert!(!snapshot.syntax().diagnostics().is_empty());
    assert_eq!(snapshot.syntax().files()[0].functions().len(), 1);
    snapshot.revalidate().expect("source retained during recovery");
}

#[test]
fn native_discovery_rejects_independent_wrong_hashes_graph_ids_and_edges() {
    let workspace = Workspace::new("malformed-graph");
    workspace.write(
        "main.zry",
        "import { value } from './dep.zry';\nexport function main(): i32 { return value(); }\n",
    );
    workspace.write("dep.zry", "export function value(): i32 { return 7; }\n");
    let root = workspace.root();
    for mutation in 0..7 {
        let mut source =
            capture_native_workspace_sources(&root, path("main.zry")).expect("genuine graph");
        match mutation {
            0 => source.modules[0].source_sha256[0] ^= 1,
            1 => source.graph_v3[0] ^= 1,
            2 => source.graph_v4[0] ^= 1,
            3 => source.modules[0].id = 1,
            4 => source.edges[0].target = path("main.zry"),
            5 => source.edges[0].local.push('x'),
            _ => {
                let inputs = source
                    .modules()
                    .iter()
                    .map(|record| zryna_source::SourceFileInput {
                        path: record.path().as_str().to_owned(),
                        text: source
                            .sources()
                            .source(source.sources().file_id(record.path()).expect("original id"))
                            .expect("original text")
                            .text()
                            .to_owned(),
                    })
                    .collect();
                source.sources =
                    zryna_source::SourceMap::build(inputs).expect("foreign issuing map");
            }
        }
        assert_eq!(
            code(&graph::authenticate(&source).expect_err("malformed graph rejects")),
            "ZRYNA-D3102"
        );
        assert!(
            source.verify_v3().is_err(),
            "malformed graph cannot reach complete parsing authority"
        );
    }
}

#[test]
fn independent_v4_candidate_with_omitted_import_cannot_replace_the_sealed_graph() {
    let workspace = Workspace::new("malformed-final-v4");
    workspace.write(
        "main.zry",
        "import { value } from './dep.zry';\nexport function main(): i32 { return value(); }\n",
    );
    workspace.write("dep.zry", "export function value(): i32 { return 7; }\n");
    let root = workspace.root();
    let source =
        capture_native_workspace_sources(&root, path("main.zry")).expect("genuine source closure");
    let lexed = native_lexer::lex(source.sources()).expect("original lexical stream");
    let mut raw = native_parser::v4::parse_v4_candidate(source.sources(), &lexed)
        .expect("genuine complete candidate");
    raw.files
        .iter_mut()
        .find(|file| file.path == "main.zry")
        .expect("entry candidate")
        .imports
        .clear();
    assert!(
        crate::ownership_closure::seal_native_closure(
            source.entrypoint.clone(),
            source.sources.clone(),
            raw,
            &source.modules,
            &source.edges,
            source.graph_v4
        )
        .is_err()
    );
    source.verify_v4().expect("unchanged complete candidate still verifies after rejected input");
}
