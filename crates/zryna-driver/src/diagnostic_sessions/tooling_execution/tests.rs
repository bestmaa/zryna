use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap};

use crate::diagnostic_sessions::{DiagnosticSession, ToolingCompiler};

use super::{
    capture::{CapturedToolingClosure, V4_MODULES},
    stage::ToolingStage,
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct CompilerFixture {
    root: PathBuf,
}

impl CompilerFixture {
    fn create() -> Self {
        let root = env::temp_dir().join(format!(
            "zryna-tooling-closure-test-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("unique tooling fixture root");
        let fixture = Self { root };
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        copy(&repository, &fixture.root, "adapters/typescript-6/src/worker.mjs");
        copy(&repository, &fixture.root, "adapters/typescript-6/src/worker-v3.mjs");
        copy(&repository, &fixture.root, "adapters/typescript-6/src/limits-v3.mjs");
        copy(&repository, &fixture.root, "adapters/typescript-6/src/worker-v4.mjs");
        copy(&repository, &fixture.root, "adapters/typescript-6/src/limits-v4.mjs");
        for module in &V4_MODULES {
            copy(&repository, &fixture.root, &format!("adapters/typescript-6/src/{}", module.path));
        }
        for relative in [
            "node_modules/.pnpm/@typescript+typescript6@6.0.2/node_modules/@typescript/typescript6/package.json",
            "node_modules/.pnpm/@typescript+typescript6@6.0.2/node_modules/@typescript/typescript6/lib/typescript.js",
            "node_modules/.pnpm/typescript@6.0.3/node_modules/typescript/package.json",
            "node_modules/.pnpm/typescript@6.0.3/node_modules/typescript/lib/typescript.js",
        ] {
            copy(&repository, &fixture.root, relative);
        }
        let adapter_link =
            fixture.root.join("adapters/typescript-6/node_modules/@typescript/typescript6");
        fs::create_dir_all(adapter_link.parent().expect("adapter link parent"))
            .expect("adapter link parent");
        let wrapper = fixture.root.join(
            "node_modules/.pnpm/@typescript+typescript6@6.0.2/node_modules/@typescript/typescript6",
        );
        create_directory_link(&wrapper, &adapter_link);
        fixture
    }
}

impl Drop for CompilerFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn copy(repository: &Path, fixture: &Path, relative: &str) {
    let destination = fixture.join(relative);
    fs::create_dir_all(destination.parent().expect("fixture parent")).expect("fixture directory");
    fs::copy(repository.join(relative), destination).expect("pinned fixture file");
}

#[cfg(unix)]
fn create_directory_link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).expect("fixture package link");
}

#[cfg(windows)]
fn create_directory_link(target: &Path, link: &Path) {
    match std::os::windows::fs::symlink_dir(target, link) {
        Ok(()) => {}
        Err(error) if error.raw_os_error() == Some(1314) => {
            fs::create_dir_all(link.join("lib")).expect("hard-linked fixture package directory");
            fs::hard_link(target.join("package.json"), link.join("package.json"))
                .expect("fixture manifest identity");
            fs::hard_link(target.join("lib/typescript.js"), link.join("lib/typescript.js"))
                .expect("fixture runtime identity");
        }
        Err(error) => panic!("fixture package link: {error}"),
    }
}

#[cfg(not(any(unix, windows)))]
fn create_directory_link(_target: &Path, _link: &Path) {
    panic!("tooling closure is supported only on Unix and Windows");
}

fn pinned_node() -> PathBuf {
    let executable = if cfg!(windows) { "node.exe" } else { "node" };
    ["ZRYNA_TEST_NODE", "NODE"]
        .into_iter()
        .filter_map(env::var_os)
        .map(PathBuf::from)
        .chain(
            env::var_os("PATH")
                .into_iter()
                .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
                .map(move |directory| directory.join(executable)),
        )
        .find(|path| path.is_file())
        .expect("pinned Node.js 22.22.1 must be configured")
        .canonicalize()
        .expect("pinned Node.js path")
}

#[test]
fn installed_pnpm_junction_resolves_to_the_pinned_graph() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root");
    CapturedToolingClosure::capture(&repository).expect("installed pnpm package graph");
}

#[test]
fn post_capture_substitute_and_deleted_dependency_never_execute() {
    let fixture = CompilerFixture::create();
    let compiler = ToolingCompiler::discover(&fixture.root, &pinned_node())
        .expect("pinned tooling closure must be captured");
    let marker = fixture.root.join("substitute-executed");
    fs::write(
        fixture.root.join("adapters/typescript-6/src/worker.mjs"),
        format!(
            "import fs from 'node:fs'; fs.writeFileSync({}, 'executed');\n",
            serde_json::to_string(&marker).expect("marker path")
        ),
    )
    .expect("replace original worker after capture");
    fs::remove_file(
        fixture
            .root
            .join("node_modules/.pnpm/typescript@6.0.3/node_modules/typescript/lib/typescript.js"),
    )
    .expect("delete original implementation after capture");

    let sources = SourceMap::build(vec![SourceFileInput {
        path: "src/main.zry".to_owned(),
        text: "export function identity(value: i32): i32 { return value; }\n".to_owned(),
    }])
    .expect("valid tooling source");
    let mut session = DiagnosticSession::try_new().expect("diagnostic session");
    compiler.admit(&mut session, sources).expect("staged worker admission");
    assert!(!marker.exists(), "the replacement worker must never execute");

    let marker_v3 = fixture.root.join("v3-substitute-executed");
    fs::write(
        fixture.root.join("adapters/typescript-6/src/worker-v3.mjs"),
        format!(
            "import fs from 'node:fs'; fs.writeFileSync({}, 'executed');\n",
            serde_json::to_string(&marker_v3).expect("marker path")
        ),
    )
    .expect("replace original v3 worker after capture");
    fs::write(fixture.root.join("adapters/typescript-6/src/limits-v3.mjs"), "throw 1;\n")
        .expect("replace original v3 limits after capture");
    let sources = SourceMap::build(vec![SourceFileInput {
        path: "src/main.zry".to_owned(),
        text: "export function identity(value: i32): i32 { let result: i32 = value; return result; }\n"
            .to_owned(),
    }])
    .expect("valid M2 source");
    let entry = NormalizedSourcePath::new("src/main.zry").expect("entry path");
    let mut session = DiagnosticSession::try_new().expect("M2 diagnostic session");
    let revision = compiler
        .admit_control_flow(&mut session, sources, &entry)
        .expect("staged v3 worker admission");
    let request = serde_json::json!({
        "query_version": 1,
        "request_id": "v3",
        "snapshot": revision.handle().to_string(),
        "revision": revision.revision(),
        "method": "diagnostics",
        "params": {},
        "limits": {"work": 100_000, "results": 10_000},
    });
    let now = std::time::Instant::now();
    let pending = session
        .begin_diagnostics(&serde_json::to_vec(&request).expect("request"), now)
        .expect("M2 diagnostic query");
    let response = session.finish_diagnostics(pending, now);
    assert_eq!(response.status(), crate::diagnostic_sessions::QueryStatus::Ok);
    let json: serde_json::Value =
        serde_json::from_str(response.encoded().expect("report")).expect("JSON report");
    assert_eq!(json["result"]["report"]["diagnostics"], serde_json::json!([]));
    assert!(!marker_v3.exists(), "the replacement v3 worker must never execute");

    let marker_v4 = fixture.root.join("v4-substitute-executed");
    fs::write(
        fixture.root.join("adapters/typescript-6/src/worker-v4.mjs"),
        format!(
            "import fs from 'node:fs'; fs.writeFileSync({}, 'executed');\n",
            serde_json::to_string(&marker_v4).expect("marker path")
        ),
    )
    .expect("replace original v4 worker after capture");
    fs::write(fixture.root.join("adapters/typescript-6/src/limits-v4.mjs"), "throw 1;\n")
        .expect("replace original v4 limits after capture");
    for module in &V4_MODULES {
        fs::write(
            fixture.root.join("adapters/typescript-6/src").join(module.path),
            "throw new Error('module replacement must never execute');\n",
        )
        .expect("replace original v4 module after capture");
    }
    let sources = SourceMap::build(vec![SourceFileInput {
        path: "src/main.zry".to_owned(),
        text: "export function identity(value: i32): i32 { return value; }\n".to_owned(),
    }])
    .expect("valid M3 source");
    let mut session = DiagnosticSession::try_new().expect("M3 diagnostic session");
    let revision = compiler
        .admit_data_ownership(&mut session, sources, &entry)
        .expect("staged v4 worker admission");
    assert!(session.format_source(revision, "src/main.zry", None).is_ok());
    assert!(!marker_v4.exists(), "the replacement v4 worker must never execute");
}

#[test]
fn capture_rejects_dependency_mapping_drift_and_worker_plus_one() {
    let fixture = CompilerFixture::create();
    let manifest = fixture.root.join(
        "node_modules/.pnpm/@typescript+typescript6@6.0.2/node_modules/@typescript/typescript6/package.json",
    );
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).expect("wrapper manifest"))
            .expect("wrapper JSON");
    value["dependencies"]["@typescript/old"] = serde_json::json!("typescript@6.0.3");
    fs::write(&manifest, serde_json::to_vec(&value).expect("changed manifest"))
        .expect("dependency drift");
    assert!(CapturedToolingClosure::capture(&fixture.root).is_err());

    let fixture = CompilerFixture::create();
    fs::write(
        fixture.root.join("adapters/typescript-6/src/worker.mjs"),
        vec![b'x'; 128 * 1_024 + 1],
    )
    .expect("worker over limit");
    assert!(CapturedToolingClosure::capture(&fixture.root).is_err());
}

#[test]
fn changed_stage_and_foreign_cleanup_entry_fail_closed() {
    let fixture = CompilerFixture::create();
    let captured = CapturedToolingClosure::capture(&fixture.root).expect("captured closure");
    let stage = ToolingStage::create(&captured).expect("private stage");
    let path = stage.physical_path().to_path_buf();
    if fs::write(path.join("worker.mjs"), "throw new Error('substitute');\n").is_ok() {
        assert!(stage.revalidate().is_err(), "changed staged bytes must reject");
        drop(stage);
        if let Err(error) = fs::remove_dir_all(path) {
            assert_eq!(
                error.kind(),
                std::io::ErrorKind::NotFound,
                "test-owned changed stage cleanup"
            );
        }
    } else {
        stage.revalidate().expect("denied replacement preserves the stage");
        drop(stage);
        assert!(!path.exists(), "unchanged stage must clean up");
    }

    let stage = ToolingStage::create(&captured).expect("private stage");
    let path = stage.physical_path().to_path_buf();
    let foreign = path.join("foreign-entry");
    fs::write(&foreign, "retain").expect("foreign stage entry");
    assert!(stage.revalidate().is_err());
    drop(stage);
    assert!(foreign.exists(), "unknown cleanup entries must be retained");
    fs::remove_dir_all(path).expect("test-owned retained stage cleanup");
}

#[test]
fn capture_rejects_each_missing_or_changed_v4_module() {
    let fixture = CompilerFixture::create();
    for module in &V4_MODULES {
        let path = fixture.root.join("adapters/typescript-6/src").join(module.path);
        let original = fs::read(&path).expect("original module bytes");
        fs::write(&path, b"untrusted module substitute").expect("change module");
        assert!(CapturedToolingClosure::capture(&fixture.root).is_err(), "{}", module.path);
        fs::remove_file(&path).expect("remove module");
        assert!(CapturedToolingClosure::capture(&fixture.root).is_err(), "{}", module.path);
        fs::write(&path, original).expect("restore module");
    }
    CapturedToolingClosure::capture(&fixture.root).expect("restored complete closure");
}

#[test]
fn source_capture_never_selects_the_historical_installed_form() {
    let fixture = CompilerFixture::create();
    fs::write(
        fixture.root.join("adapters/typescript-6/src/worker-v4.mjs"),
        super::installed_tests::legacy_worker(),
    )
    .expect("historical source fixture");
    assert!(CapturedToolingClosure::capture(&fixture.root).is_err());
}

#[test]
fn staged_v4_module_mutation_and_unknown_directory_entries_fail_closed() {
    let fixture = CompilerFixture::create();
    let captured = CapturedToolingClosure::capture(&fixture.root).expect("captured closure");
    for module in &V4_MODULES {
        let stage = ToolingStage::create(&captured).expect("private stage");
        let path = stage.physical_path().to_path_buf();
        if fs::write(path.join(module.path), "throw new Error('substitute');\n").is_ok() {
            assert!(stage.revalidate().is_err(), "{}", module.path);
            drop(stage);
            if path.exists() {
                fs::remove_dir_all(path).expect("test-owned changed stage cleanup");
            }
        } else {
            stage.revalidate().expect("denied replacement preserves stage");
            drop(stage);
            assert!(!path.exists(), "unchanged nested stage must clean up");
        }
    }
    for directory in ["v4", "v4/boundary", "v4/syntax"] {
        let stage = ToolingStage::create(&captured).expect("private stage");
        let path = stage.physical_path().to_path_buf();
        let foreign = path.join(directory).join("foreign-entry");
        fs::write(&foreign, "retain").expect("foreign nested entry");
        assert!(stage.revalidate().is_err(), "{directory}");
        drop(stage);
        assert!(foreign.exists(), "unknown nested entries must be retained");
        fs::remove_dir_all(path).expect("test-owned retained nested cleanup");
    }
}
