//! Executable-level acceptance tests for the public M1 CLI contract.

#![forbid(unsafe_code)]

use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(unix)]
use std::process::{Child, Stdio};

use serde_json::{Value, json};
use sha2::Digest;
use zryna_abi::{RawHostScalar, ScalarTarget, ScalarType, ScalarValue, normalize_result};

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

struct WorkspaceCase {
    root: PathBuf,
    source_relative: String,
    owned_paths: Vec<PathBuf>,
    unique: String,
}

impl WorkspaceCase {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repository workspace root must resolve");
        let sequence = NEXT_CASE.fetch_add(1, Ordering::Relaxed);
        let unique = format!("zryna_cli_{}_{}", std::process::id(), sequence);
        let source_relative = format!(".zryna/cache/{unique}/main.zry");
        let source = root.join(&source_relative);
        fs::create_dir_all(source.parent().expect("source parent must exist"))
            .expect("test source directory must be created");
        fs::write(&source, include_str!("../../../examples/universal/add.zry"))
            .expect("test source must be written");
        Self { root, source_relative, owned_paths: Vec::new(), unique }
    }

    fn control_flow() -> Self {
        let mut case = Self::new();
        "examples/control-flow/main.zry".clone_into(&mut case.source_relative);
        case
    }

    fn stem(&mut self, label: &str, command: &str) -> String {
        let stem = format!("{}_{}", self.unique, label);
        self.owned_paths.push(self.root.join(".zryna/out").join(format!("{stem}.{command}")));
        stem
    }

    fn missing_entrypoint(&self) -> String {
        format!(".zryna/cache/{}/missing.zry", self.unique)
    }

    fn bundle(&self, stem: &str, command: &str) -> PathBuf {
        self.root.join(".zryna/out").join(format!("{stem}.{command}"))
    }
}

impl Drop for WorkspaceCase {
    fn drop(&mut self) {
        for path in &self.owned_paths {
            let _ = fs::remove_dir_all(path);
        }
        let _ = fs::remove_dir_all(self.root.join(".zryna/cache").join(&self.unique));
    }
}

struct InvalidWorkspace {
    root: PathBuf,
}

impl InvalidWorkspace {
    fn new() -> Self {
        let sequence = NEXT_CASE.fetch_add(1, Ordering::Relaxed);
        let root =
            env::temp_dir().join(format!("zryna-cli-invalid-{}-{sequence}", std::process::id()));
        fs::create_dir_all(root.join("src")).expect("invalid fixture directory must be created");
        fs::write(root.join("src/main.zry"), "export function value(): i32 { return 1; }")
            .expect("invalid fixture source must be written");
        Self { root }
    }
}

impl Drop for InvalidWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct ProjectCase {
    parent: PathBuf,
    root: PathBuf,
}

impl ProjectCase {
    fn empty(name: &str) -> Self {
        let sequence = NEXT_CASE.fetch_add(1, Ordering::Relaxed);
        let parent =
            env::temp_dir().join(format!("zryna-project-cli-{}-{sequence}", std::process::id()));
        fs::create_dir(&parent).expect("project test parent must be created");
        let root = parent.join(name);
        Self { parent, root }
    }

    fn create(name: &str) -> Self {
        let case = Self::empty(name);
        let output =
            zryna().arg("new").arg(&case.root).output().expect("project creation must start");
        assert_success(&output);
        case
    }

    fn public_predecessor(version: &str) -> Self {
        let case = Self::empty("hello");
        fs::create_dir_all(case.root.join("src")).expect("legacy project source directory");
        let files: [(&str, &[u8]); 3] = match version {
            "0.2.1" => [
                (
                    "zryna.package.json",
                    include_bytes!("fixtures/public-v0.2.1-hello/zryna.package.json"),
                ),
                ("zryna.lock.json", include_bytes!("fixtures/public-v0.2.1-hello/zryna.lock.json")),
                ("src/main.zry", include_bytes!("fixtures/public-v0.2.1-hello/src/main.zry")),
            ],
            "0.2.2" => [
                (
                    "zryna.package.json",
                    include_bytes!("fixtures/public-v0.2.2-hello/zryna.package.json"),
                ),
                ("zryna.lock.json", include_bytes!("fixtures/public-v0.2.2-hello/zryna.lock.json")),
                ("src/main.zry", include_bytes!("fixtures/public-v0.2.2-hello/src/main.zry")),
            ],
            _ => panic!("unsupported public predecessor fixture {version}"),
        };
        for (relative, bytes) in files {
            fs::write(case.root.join(relative), bytes)
                .unwrap_or_else(|_| panic!("exact public v{version} scaffold file"));
        }
        case
    }

    fn compiler_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("compiler root must resolve")
    }

    fn command(&self, arguments: &[&str]) -> Output {
        zryna()
            .args(arguments)
            .arg("--root")
            .arg(Self::compiler_root())
            .arg("--project-root")
            .arg(&self.root)
            .arg("--node")
            .arg(node_executable())
            .output()
            .expect("standalone project command must start")
    }
}

impl Drop for ProjectCase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.parent);
    }
}

fn zryna() -> Command {
    Command::new(env!("CARGO_BIN_EXE_zryna"))
}

fn node_executable() -> PathBuf {
    let configured = ["ZRYNA_TEST_NODE", "NODE"]
        .into_iter()
        .filter_map(env::var_os)
        .map(PathBuf::from)
        .chain({
            let executable = if cfg!(windows) { "node.exe" } else { "node" };
            env::var_os("PATH")
                .into_iter()
                .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
                .map(move |directory| directory.join(executable))
        })
        .find_map(|candidate| candidate.is_file().then(|| candidate.canonicalize().ok()).flatten())
        .expect("Node.js 22.22.1 must be available to CLI integration tests");
    let output = Command::new(&configured)
        .arg("--version")
        .output()
        .expect("Node.js version probe must start");
    assert!(output.status.success(), "Node.js version probe must succeed");
    assert!(
        matches!(output.stdout.as_slice(), b"v22.22.1\n" | b"v22.22.1\r\n"),
        "Node.js version probe must report exactly v22.22.1"
    );
    assert!(output.stderr.is_empty());
    configured
}

fn dummy_absolute_node() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\zryna-test-missing-node.exe")
    } else {
        PathBuf::from("/zryna-test-missing-node")
    }
}

fn command_output(case: &WorkspaceCase, arguments: &[&str]) -> Output {
    zryna()
        .args(arguments)
        .arg("--root")
        .arg(&case.root)
        .arg("--node")
        .arg(node_executable())
        .output()
        .expect("zryna CLI must start")
}

#[cfg(unix)]
fn compile_test_node(case: &WorkspaceCase, label: &str, target_body: &str) -> PathBuf {
    let wrapper =
        case.root.join(".zryna/cache").join(&case.unique).join(format!("node-wrapper-{label}"));
    let wrapper_source = wrapper.with_extension("rs");
    let real_node = node_executable();
    let real_node_literal = serde_json::to_string(real_node.to_string_lossy().as_ref())
        .expect("Node path must serialize as a source literal");
    fs::write(
        &wrapper_source,
        format!(
            r#"fn main() {{
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.first().is_some_and(|value| value == "--version") {{
        print!("v22.22.1\n");
        return;
    }}
    if arguments.first().and_then(|value| std::path::Path::new(value).file_name()).is_some_and(|value| value == "worker.mjs") {{
        let status = std::process::Command::new({real_node_literal}).args(&arguments).status().expect("real Node must start");
        std::process::exit(status.code().unwrap_or(1));
    }}
    {target_body}
}}
"#
        ),
    )
    .expect("runtime wrapper source must be written");
    let compile = Command::new("rustc")
        .args(["--edition=2024", "-o"])
        .arg(&wrapper)
        .arg(&wrapper_source)
        .output()
        .expect("runtime wrapper compiler must start");
    assert_success(&compile);
    wrapper
}

#[cfg(unix)]
fn compile_mutating_project_node(case: &ProjectCase, marker: &Path) -> PathBuf {
    let wrapper = case.parent.join("mutating-node");
    let wrapper_source = wrapper.with_extension("rs");
    let real_node = serde_json::to_string(node_executable().to_string_lossy().as_ref())
        .expect("real Node path literal");
    let source = serde_json::to_string(case.root.join("src/main.zry").to_string_lossy().as_ref())
        .expect("source path literal");
    let marker =
        serde_json::to_string(marker.to_string_lossy().as_ref()).expect("marker path literal");
    fs::write(
        &wrapper_source,
        format!(
            r#"fn main() {{
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.first().is_some_and(|value| value == "--version") {{
        print!("v22.22.1\n");
        return;
    }}
    if arguments.first().and_then(|value| std::path::Path::new(value).file_name()).is_some_and(|value| value == "worker.mjs") {{
        let status = std::process::Command::new({real_node}).args(&arguments).status().expect("real Node must start");
        if status.success() {{
            std::fs::write({source}, b"export function main(): i32 {{ return 7; }}\n").expect("mutate source");
        }}
        std::process::exit(status.code().unwrap_or(1));
    }}
    std::fs::write({marker}, b"executed").expect("execution marker");
    std::process::exit(1);
}}
"#
        ),
    )
    .expect("mutating runtime wrapper source");
    let compile = Command::new("rustc")
        .args(["--edition=2024", "-o"])
        .arg(&wrapper)
        .arg(&wrapper_source)
        .output()
        .expect("mutating runtime wrapper compiler must start");
    assert_success(&compile);
    wrapper
}

#[cfg(unix)]
struct TestChildGuard {
    child: Option<Child>,
    release: PathBuf,
    owned_paths: Vec<PathBuf>,
}

#[cfg(unix)]
impl TestChildGuard {
    fn new(child: Child, release: PathBuf) -> Self {
        Self { child: Some(child), release, owned_paths: Vec::new() }
    }

    fn id(&self) -> u32 {
        self.child.as_ref().expect("test child must still be owned").id()
    }

    fn own_path(&mut self, path: PathBuf) {
        self.owned_paths.push(path);
    }

    fn has_exited(&mut self) -> bool {
        self.child
            .as_mut()
            .expect("test child must still be owned")
            .try_wait()
            .expect("cleanup-failure child status must be readable")
            .is_some()
    }

    fn release(&self) {
        match fs::create_dir(&self.release) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                assert!(
                    self.release.is_dir(),
                    "existing release marker must be a test-owned directory"
                );
            }
            Err(error) => panic!("release marker must be created: {error}"),
        }
    }

    fn release_best_effort(&self) {
        let _ = fs::create_dir(&self.release);
    }

    fn wait_with_output(&mut self) -> Output {
        self.release();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
        loop {
            match self.child.as_mut().expect("test child must still be owned").try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Ok(None) => panic!("cleanup-failure command exceeded the test deadline"),
                Err(error) => panic!("cleanup-failure child status must be readable: {error}"),
            }
        }
        self.child
            .take()
            .expect("test child must still be owned")
            .wait_with_output()
            .expect("cleanup-failure command must finish")
    }
}

#[cfg(unix)]
impl Drop for TestChildGuard {
    fn drop(&mut self) {
        self.release_best_effort();
        if let Some(mut child) = self.child.take() {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Ok(None) | Err(_) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                }
            }
        }
        for path in self.owned_paths.iter().rev() {
            let _ = fs::remove_dir_all(path);
        }
        let _ = fs::remove_dir(&self.release);
    }
}

#[cfg(unix)]
fn wait_for_test_marker(path: &Path, child: &mut TestChildGuard) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match fs::metadata(path) {
            Ok(metadata) => {
                assert!(metadata.is_dir(), "runtime marker must be a test-owned directory");
                return;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("runtime marker must be readable: {error}"),
        }
        if child.has_exited() {
            let output = child.wait_with_output();
            panic!(
                "cleanup-failure command exited before runtime readiness\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert!(
            std::time::Instant::now() < deadline,
            "runtime marker did not appear before the test deadline"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "successful command wrote stderr");
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("JSON file must be readable"))
        .expect("JSON file must decode")
}

#[test]
fn component_build_publishes_one_audited_build_only_artifact() {
    let mut case = WorkspaceCase::new();
    let stem = case.stem("component_build", "build");
    let output = command_output(
        &case,
        &["build", &case.source_relative, "--target", "component", "--name", &stem],
    );
    assert_success(&output);
    let bundle = case.bundle(&stem, "build");
    let artifact_path = bundle.join(format!("component/{stem}.wasm"));
    let bytes = fs::read(&artifact_path).expect("component artifact");
    assert_eq!(&bytes[..8], b"\0asm\r\0\x01\0");
    assert!(!bundle.join("webassembly").exists());
    let manifest = read_json(&bundle.join("zryna-manifest-v1.json"));
    assert_eq!(manifest["targets"], json!(["component"]));
    assert_eq!(manifest["artifacts"][0]["target"], "component");
    assert_eq!(manifest["artifacts"][0]["kind"], "webassembly-component");
    assert_eq!(manifest["artifacts"][0]["path"], format!("component/{stem}.wasm"));
    assert_eq!(manifest["artifacts"][0]["bytes"], bytes.len());

    let run_stem = case.stem("component_run", "run");
    let rejected = command_output(
        &case,
        &[
            "run",
            &case.source_relative,
            "--target",
            "component",
            "--name",
            &run_stem,
            "--export",
            "add",
            "--arg=i32:20",
            "--arg=i32:22",
            "--json",
        ],
    );
    assert_eq!(rejected.status.code(), Some(2));
    let response: Value =
        serde_json::from_slice(&rejected.stdout).expect("component rejection JSON");
    assert_eq!(response["diagnostics"][0]["code"], "ZRYNA-C1012");
    assert!(!case.bundle(&run_stem, "run").exists());

    let profile_stem = case.stem("component_profile", "build");
    let rejected = command_output(
        &case,
        &[
            "build",
            &case.source_relative,
            "--profile",
            "control-flow-v1",
            "--target",
            "component",
            "--name",
            &profile_stem,
            "--json",
        ],
    );
    assert_eq!(rejected.status.code(), Some(2));
    let response: Value =
        serde_json::from_slice(&rejected.stdout).expect("component profile rejection JSON");
    assert_eq!(response["diagnostics"][0]["code"], "ZRYNA-C1012");
    assert!(!case.bundle(&profile_stem, "build").exists());
}

fn browser_component_case() -> (WorkspaceCase, PathBuf) {
    let mut case = WorkspaceCase::new();
    let stem = case.stem("browser_component", "build");
    let output = command_output(
        &case,
        &[
            "build",
            &case.source_relative,
            "--profile",
            "browser-component-v1",
            "--target",
            "component",
            "--name",
            &stem,
        ],
    );
    assert_success(&output);
    let bundle = case.bundle(&stem, "build");
    let manifest = read_json(&bundle.join("zryna-browser-manifest-v1.json"));
    assert_eq!(manifest["profile"], "zryna-browser-component-v1");
    assert_eq!(manifest["targets"], json!(["component"]));
    assert_eq!(manifest["browser"]["world"], "zryna:capability-profiles/browser@0.1.0");
    assert_eq!(manifest["browser"]["component_sha256"], manifest["artifacts"][0]["sha256"]);
    let offset = usize::try_from(manifest["browser"]["core_offset"].as_u64().expect("core offset"))
        .expect("bounded core offset");
    let component =
        fs::read(bundle.join(format!("component/{stem}.wasm"))).expect("component bytes");
    assert_eq!(&component[offset..offset + 8], b"\0asm\x01\0\0\0");
    let paths = manifest["artifacts"]
        .as_array()
        .expect("artifact inventory")
        .iter()
        .map(|record| record["path"].as_str().expect("path").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            format!("component/{stem}.wasm"),
            format!("component/{stem}.mjs"),
            format!("component/{stem}.d.mts"),
        ]
    );
    let actual = fs::read_dir(bundle.join("component"))
        .expect("component directory")
        .map(|entry| entry.expect("entry").file_name().into_string().expect("portable name"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual,
        [format!("{stem}.wasm"), format!("{stem}.mjs"), format!("{stem}.d.mts")].into()
    );
    for artifact in manifest["artifacts"].as_array().expect("artifacts") {
        let path = artifact["path"].as_str().expect("path");
        let bytes = fs::read(bundle.join(path)).expect("published artifact");
        assert_eq!(artifact["bytes"], bytes.len());
        assert_eq!(artifact["sha256"], format!("{:x}", sha2::Sha256::digest(&bytes)));
    }
    (case, bundle)
}

fn run_browser_component_fixture(browser_root: Option<&Path>) {
    let (case, bundle) = browser_component_case();
    let stem = bundle.file_stem().expect("bundle stem").to_string_lossy();
    let node_path = |path: &Path| {
        let rendered = path.to_string_lossy();
        if cfg!(windows) {
            rendered.trim_start_matches(r"\\?\").to_owned()
        } else {
            rendered.into_owned()
        }
    };
    let output = Command::new(node_executable())
        .arg(node_path(&case.root.join("tests/browser-component/run.mjs")))
        .arg(node_path(&bundle.join("component").join(format!("{stem}.mjs"))))
        .arg(node_path(&bundle.join("component").join(format!("{stem}.wasm"))))
        .args(browser_root.map(&node_path))
        .current_dir(&case.root)
        .output()
        .expect("browser component fixture must start");
    assert_success(&output);
}

#[test]
fn browser_component_bundle_and_node_bindings_are_sealed() {
    run_browser_component_fixture(None);
}

#[test]
fn browser_component_replays_deterministically_and_rejects_stale_binding() {
    let mut case = WorkspaceCase::new();
    let first = case.stem("browser_first", "build");
    let second = case.stem("browser_second", "build");
    for stem in [&first, &second] {
        let output = command_output(
            &case,
            &[
                "build",
                &case.source_relative,
                "--profile",
                "browser-component-v1",
                "--target",
                "component",
                "--name",
                stem,
            ],
        );
        assert_success(&output);
    }
    for extension in ["wasm", "mjs", "d.mts"] {
        let first_bytes =
            fs::read(case.bundle(&first, "build").join(format!("component/{first}.{extension}")))
                .expect("first artifact");
        let second_bytes =
            fs::read(case.bundle(&second, "build").join(format!("component/{second}.{extension}")))
                .expect("second artifact");
        assert_eq!(first_bytes, second_bytes, "{extension} must replay exactly");
    }
    fs::write(
        case.root.join(&case.source_relative),
        "export function add(a: i32, b: i32): i32 { return a + a; }\n",
    )
    .expect("changed source");
    let changed = case.stem("browser_changed", "build");
    let output = command_output(
        &case,
        &[
            "build",
            &case.source_relative,
            "--profile",
            "browser-component-v1",
            "--target",
            "component",
            "--name",
            &changed,
        ],
    );
    assert_success(&output);
    let loader = case.bundle(&first, "build").join(format!("component/{first}.mjs"));
    let component = case.bundle(&changed, "build").join(format!("component/{changed}.wasm"));
    let script = "import assert from 'node:assert/strict'; import {readFile} from 'node:fs/promises'; import {pathToFileURL} from 'node:url'; const {instantiateBrowserComponent} = await import(pathToFileURL(process.argv[1])); await assert.rejects(instantiateBrowserComponent(new Uint8Array(await readFile(process.argv[2]))), error => error.message === 'ZRYNA-B3992');";
    let compatible = |path: &Path| path.to_string_lossy().trim_start_matches(r"\\?\").to_owned();
    let output = Command::new(node_executable())
        .args(["--input-type=module", "--eval", script])
        .arg(compatible(&loader))
        .arg(compatible(&component))
        .output()
        .expect("stale-binding test must start");
    assert_success(&output);
}

#[test]
fn browser_component_selector_rejects_other_targets_and_run_without_a_bundle() {
    let mut case = WorkspaceCase::new();
    for (command, target) in [("build", "javascript"), ("run", "component")] {
        let stem = case.stem("browser_rejected", command);
        let mut arguments = vec![
            command,
            &case.source_relative,
            "--profile",
            "browser-component-v1",
            "--target",
            target,
            "--name",
            &stem,
            "--json",
        ];
        if command == "run" {
            arguments.extend(["--export", "add", "--arg=i32:20", "--arg=i32:22"]);
        }
        let output = command_output(&case, &arguments);
        assert_eq!(output.status.code(), Some(2));
        let response: Value = serde_json::from_slice(&output.stdout).expect("rejection JSON");
        assert_eq!(response["diagnostics"][0]["code"], "ZRYNA-C1012");
        assert!(!case.bundle(&stem, command).exists());
    }
}

#[test]
#[ignore = "requires reviewed pinned Chrome for Testing fixture"]
fn browser_component_pinned_real_browser_and_node_agree() {
    let root = env::var_os("ZRYNA_TEST_BROWSER_ROOT")
        .map(PathBuf::from)
        .expect("reviewed browser fixture root required");
    run_browser_component_fixture(Some(&root));
}

fn m1_registry() -> Value {
    serde_json::from_str(include_str!("../../../tests/m1-conformance-v1.json"))
        .expect("M1 conformance registry must be strict JSON")
}

fn object_keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .expect("registry value must be an object")
        .keys()
        .map(String::as_str)
        .collect()
}

fn m1_cases(registry: &Value) -> &[Value] {
    registry["cases"].as_array().expect("M1 cases must be an array")
}

fn m1_case_arguments(case: &Value) -> Vec<String> {
    case["arguments"]
        .as_array()
        .expect("M1 case arguments must be an array")
        .iter()
        .map(|argument| {
            assert_eq!(object_keys(argument), BTreeSet::from(["type", "value"]));
            assert_eq!(argument["type"], "i32");
            format!("--arg=i32:{}", argument["value"].as_i64().expect("i32 fixture value"))
        })
        .collect()
}

fn run_m1_case(case: &mut WorkspaceCase, fixture: &Value, target: &str, label: &str) -> Value {
    let stem = case.stem(label, "run");
    let registry = m1_registry();
    let mut arguments = vec![
        "run".to_owned(),
        case.source_relative.clone(),
        "--target".to_owned(),
        target.to_owned(),
        "--name".to_owned(),
        stem.clone(),
        "--export".to_owned(),
        registry["export"].as_str().expect("M1 export").to_owned(),
    ];
    arguments.extend(m1_case_arguments(fixture));
    arguments.push("--json".to_owned());
    let borrowed = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    let output = command_output(case, &borrowed);
    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("M1 JSON response");
    assert_eq!(
        object_keys(&response),
        BTreeSet::from(["command", "diagnostics", "manifest", "ok", "results", "version"])
    );
    assert_eq!(response["version"], 1);
    assert_eq!(response["ok"], true);
    assert_eq!(response["command"], "run");
    assert_eq!(response["diagnostics"], json!([]));
    assert_eq!(response["manifest"], format!(".zryna/out/{stem}.run/zryna-manifest-v1.json"));
    response
}

fn assert_m1_typed_results(response: &Value, targets: &[&str], expected: i64) {
    let expected_results = targets
        .iter()
        .map(|target| {
            json!({
                "target": target,
                "outcome": {
                    "kind": "returned",
                    "value": {"type": "i32", "value": expected}
                }
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(response["results"], json!(expected_results));
    let outcomes = response["results"]
        .as_array()
        .expect("M1 results")
        .iter()
        .map(|result| result["outcome"].clone())
        .collect::<Vec<_>>();
    assert!(outcomes.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn m1_registry_freezes_the_supported_slice() {
    let registry = m1_registry();
    assert_eq!(
        object_keys(&registry),
        BTreeSet::from([
            "cases",
            "entrypoint",
            "export",
            "gatedBooleanSource",
            "invalidSource",
            "profile",
            "schemaVersion",
            "targets",
        ])
    );
    assert_eq!(registry["schemaVersion"], 1);
    assert_eq!(registry["profile"], "zryna-m1-i32-differential-v1");
    assert_eq!(registry["entrypoint"], "examples/universal/add.zry");
    assert_eq!(registry["export"], "add");
    assert_eq!(registry["targets"], json!(["javascript", "webassembly", "native"]));
    let cases = m1_cases(&registry);
    assert_eq!(
        cases.iter().map(|case| case["id"].as_str().expect("M1 case id")).collect::<Vec<_>>(),
        ["one-plus-two", "maximum-plus-one-wraps", "minimum-minus-one-wraps"]
    );
    assert_eq!(
        cases
            .iter()
            .map(|case| {
                let arguments = case["arguments"]
                    .as_array()
                    .expect("M1 arguments")
                    .iter()
                    .map(|argument| argument["value"].as_i64().expect("M1 argument value"))
                    .collect::<Vec<_>>();
                (arguments, case["expected"]["value"].as_i64().expect("M1 expected value"))
            })
            .collect::<Vec<_>>(),
        [
            (vec![1, 2], 3),
            (vec![i64::from(i32::MAX), 1], i64::from(i32::MIN)),
            (vec![i64::from(i32::MIN), -1], i64::from(i32::MAX)),
        ]
    );
    for case in cases {
        assert_eq!(object_keys(case), BTreeSet::from(["arguments", "expected", "id"]));
        assert_eq!(object_keys(&case["expected"]), BTreeSet::from(["type", "value"]));
        assert_eq!(case["expected"]["type"], "i32");
        assert_eq!(case["arguments"].as_array().map(Vec::len), Some(2));
    }
    assert_eq!(
        registry["invalidSource"],
        json!({"path": "tests/m1-fixtures/invalid-any.zry", "expectedCode": "ZRYNA-M1004"})
    );
    assert_eq!(
        registry["gatedBooleanSource"],
        json!({"path": "tests/m1-fixtures/bool-gated.zry", "expectedCode": "ZRYNA-I1006"})
    );
}

#[test]
fn m1_javascript_and_webassembly_match_every_portable_fixture() {
    let registry = m1_registry();
    let mut workspace = WorkspaceCase::new();
    for fixture in m1_cases(&registry) {
        let id = fixture["id"].as_str().expect("M1 case id");
        let expected = fixture["expected"]["value"].as_i64().expect("M1 expected i32");
        for target in ["javascript", "webassembly"] {
            let response =
                run_m1_case(&mut workspace, fixture, target, &format!("m1_{id}_{target}"));
            assert_m1_typed_results(&response, &[target], expected);
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
#[test]
fn m1_linux_three_target_results_and_artifacts_match_every_fixture() {
    let registry = m1_registry();
    let targets = ["javascript", "webassembly", "native"];
    let mut workspace = WorkspaceCase::new();
    for fixture in m1_cases(&registry) {
        let id = fixture["id"].as_str().expect("M1 case id");
        let expected = fixture["expected"]["value"].as_i64().expect("M1 expected i32");
        let label = format!("m1_{id}_all");
        let response = run_m1_case(&mut workspace, fixture, "all", &label);
        assert_m1_typed_results(&response, &targets, expected);

        let stem = format!("{}_{}", workspace.unique, label);
        let bundle = workspace.bundle(&stem, "run");
        let manifest = read_json(&bundle.join("zryna-manifest-v1.json"));
        assert_eq!(manifest["targets"], registry["targets"]);
        assert_eq!(
            manifest["results"],
            json!(targets.map(|target| json!({
                "target": target,
                "outcome": {"kind": "returned", "type": "i32", "value": expected}
            })))
        );
        assert_eq!(
            fs::read_dir(&bundle)
                .expect("M1 bundle")
                .map(|entry| entry
                    .expect("bundle entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "javascript".to_owned(),
                "native".to_owned(),
                "webassembly".to_owned(),
                "zryna-manifest-v1.json".to_owned(),
            ])
        );
        for (target, extension) in
            [("javascript", "mjs"), ("webassembly", "wasm"), ("native", "elf")]
        {
            let files = fs::read_dir(bundle.join(target))
                .expect("target directory")
                .map(|entry| {
                    entry.expect("target artifact").file_name().to_string_lossy().into_owned()
                })
                .collect::<Vec<_>>();
            assert_eq!(files, [format!("{stem}.{extension}")]);
        }
    }
}

#[test]
fn m1_invalid_source_is_target_independent_and_publishes_nothing() {
    let registry = m1_registry();
    let invalid = &registry["invalidSource"];
    assert_eq!(object_keys(invalid), BTreeSet::from(["expectedCode", "path"]));
    let mut workspace = WorkspaceCase::new();
    let invalid_source = fs::read_to_string(
        workspace.root.join(invalid["path"].as_str().expect("invalid fixture path")),
    )
    .expect("invalid M1 fixture");
    fs::write(workspace.root.join(&workspace.source_relative), invalid_source)
        .expect("invalid fixture must replace the temporary source");
    let mut expected_diagnostics = None;

    for target in ["javascript", "webassembly", "native", "all"] {
        let stem = workspace.stem(&format!("m1_invalid_{target}"), "build");
        let output = command_output(
            &workspace,
            &["build", &workspace.source_relative, "--target", target, "--name", &stem, "--json"],
        );
        assert_eq!(output.status.code(), Some(3));
        assert!(output.stderr.is_empty());
        let response: Value = serde_json::from_slice(&output.stdout).expect("invalid M1 JSON");
        assert_eq!(response["ok"], false);
        assert!(response["manifest"].is_null());
        assert_eq!(response["results"], json!([]));
        assert!(
            response["diagnostics"]
                .as_array()
                .expect("invalid diagnostics")
                .iter()
                .any(|diagnostic| diagnostic["code"] == invalid["expectedCode"])
        );
        let diagnostic_bytes = serde_json::to_vec(&response["diagnostics"])
            .expect("invalid diagnostics must serialize deterministically");
        if let Some(expected) = &expected_diagnostics {
            assert_eq!(&diagnostic_bytes, expected);
        } else {
            expected_diagnostics = Some(diagnostic_bytes);
        }
        assert!(!workspace.bundle(&stem, "build").exists());
    }
}

#[test]
fn m1_bool_host_normalization_matches_without_enabling_bool_source() {
    for value in [false, true] {
        let lane = i32::from(value);
        let normalized = [
            normalize_result(
                ScalarTarget::JavaScript,
                ScalarType::Bool,
                RawHostScalar::JavaScriptBool(value),
            ),
            normalize_result(
                ScalarTarget::CoreWebAssembly,
                ScalarType::Bool,
                RawHostScalar::I32(lane),
            ),
            normalize_result(
                ScalarTarget::NativeLinuxX8664,
                ScalarType::Bool,
                RawHostScalar::I32(lane),
            ),
        ];
        assert!(normalized.iter().all(|result| *result == Ok(ScalarValue::Bool(value))));
    }
    assert!(
        normalize_result(
            ScalarTarget::JavaScript,
            ScalarType::Bool,
            RawHostScalar::JavaScriptNumber(1.0),
        )
        .is_err()
    );
    for target in [ScalarTarget::CoreWebAssembly, ScalarTarget::NativeLinuxX8664] {
        for invalid in [-1, 2] {
            assert!(
                normalize_result(target, ScalarType::Bool, RawHostScalar::I32(invalid)).is_err()
            );
        }
    }

    let registry = m1_registry();
    let gated = &registry["gatedBooleanSource"];
    assert_eq!(object_keys(gated), BTreeSet::from(["expectedCode", "path"]));
    let mut workspace = WorkspaceCase::new();
    let bool_source = fs::read_to_string(
        workspace.root.join(gated["path"].as_str().expect("Boolean fixture path")),
    )
    .expect("gated Boolean fixture");
    fs::write(workspace.root.join(&workspace.source_relative), bool_source)
        .expect("Boolean fixture must replace the temporary source");
    let mut expected_diagnostics = None;
    for target in ["javascript", "webassembly", "native", "all"] {
        let stem = workspace.stem(&format!("m1_bool_gated_{target}"), "build");
        let output = command_output(
            &workspace,
            &["build", &workspace.source_relative, "--target", target, "--name", &stem, "--json"],
        );
        assert_eq!(output.status.code(), Some(3));
        assert!(output.stderr.is_empty());
        let response: Value = serde_json::from_slice(&output.stdout).expect("Boolean gate JSON");
        assert!(
            response["diagnostics"]
                .as_array()
                .expect("Boolean diagnostics")
                .iter()
                .any(|diagnostic| diagnostic["code"] == gated["expectedCode"])
        );
        let diagnostic_bytes = serde_json::to_vec(&response["diagnostics"])
            .expect("Boolean diagnostics must serialize deterministically");
        if let Some(expected) = &expected_diagnostics {
            assert_eq!(&diagnostic_bytes, expected);
        } else {
            expected_diagnostics = Some(diagnostic_bytes);
        }
        assert!(response["manifest"].is_null());
        assert_eq!(response["results"], json!([]));
        assert!(!workspace.bundle(&stem, "build").exists());
    }
}

#[test]
fn help_and_required_target_and_node_are_stable() {
    for arguments in
        [["--help"].as_slice(), ["build", "--help"].as_slice(), ["run", "--help"].as_slice()]
    {
        let output = zryna().args(arguments).output().expect("help command must start");
        assert_eq!(output.status.code(), Some(0), "help must succeed: {arguments:?}");
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).expect("help must be UTF-8");
        assert!(stdout.contains("Usage:"));
        if arguments.first() == Some(&"build") || arguments.first() == Some(&"run") {
            assert!(stdout.contains("--profile"));
            assert!(stdout.contains("control-flow-v1"));
        }
    }

    let missing_target = zryna()
        .args(["build", "src/main.zry", "--node"])
        .arg(dummy_absolute_node())
        .output()
        .expect("missing-target command must start");
    assert_eq!(missing_target.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing_target.stderr).contains("--target"));

    let missing_node = zryna()
        .args(["build", "src/main.zry", "--target", "javascript"])
        .output()
        .expect("missing-node command must start");
    assert_eq!(missing_node.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing_node.stderr).contains("--node"));
}

#[test]
fn control_flow_profile_is_exact_and_selects_boolean_argument_parsing() {
    let invalid_profile = zryna()
        .args(["build", "src/main.zry", "--target", "javascript", "--profile", "m2", "--node"])
        .arg(dummy_absolute_node())
        .output()
        .expect("invalid-profile command must start");
    assert_eq!(invalid_profile.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid_profile.stderr).contains("control-flow-v1"));

    let workspace = InvalidWorkspace::new();
    let accepted_boolean = zryna()
        .args([
            "run",
            "src/main.zry",
            "--target",
            "javascript",
            "--profile",
            "control-flow-v1",
            "--export",
            "value",
            "--arg=bool:true",
            "--root",
        ])
        .arg(&workspace.root)
        .arg("--node")
        .arg(dummy_absolute_node())
        .output()
        .expect("control-flow Boolean command must start");
    assert_eq!(
        accepted_boolean.status.code(),
        Some(1),
        "canonical Boolean must reach the architecture gate rather than fail CLI parsing"
    );
    assert!(!String::from_utf8_lossy(&accepted_boolean.stderr).contains("typed argument"));
}

#[test]
fn control_flow_multifile_build_records_canonical_manifest_v2() {
    let mut workspace = WorkspaceCase::control_flow();
    let stem = workspace.stem("m2_multifile_all", "build");
    let output = command_output(
        &workspace,
        &[
            "build",
            &workspace.source_relative,
            "--profile",
            "control-flow-v1",
            "--target",
            "all",
            "--name",
            &stem,
            "--json",
        ],
    );
    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("M2 build response");
    assert_eq!(response["ok"], true);
    assert_eq!(response["manifest"], format!(".zryna/out/{stem}.build/zryna-manifest-v2.json"));
    let bundle = workspace.bundle(&stem, "build");
    let manifest = read_json(&bundle.join("zryna-manifest-v2.json"));
    assert_eq!(manifest["version"], 2);
    assert_eq!(manifest["profile"], "zryna-control-flow-v1");
    assert_eq!(manifest["entrypoint"], workspace.source_relative);
    assert_eq!(manifest["targets"], json!(["javascript", "webassembly", "native"]));
    assert_eq!(manifest["sources"].as_array().map(Vec::len), Some(2));
    assert_eq!(manifest["sources"][0]["id"], 0);
    assert_eq!(manifest["sources"][0]["path"], workspace.source_relative);
    assert_eq!(manifest["sources"][1]["id"], 1);
    assert!(
        manifest["sources"][1]["path"].as_str().is_some_and(|path| path.ends_with("/math.zry"))
    );
    assert_eq!(
        manifest["edges"],
        json!([{
            "importer": workspace.source_relative,
            "target": "examples/control-flow/math.zry",
            "specifier": "./math.zry",
            "imported": "double",
            "local": "double"
        }])
    );
    assert!(manifest["graph_sha256"].as_str().is_some_and(|digest| {
        digest.len() == 64
            && digest.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }));
    assert_eq!(manifest["artifacts"].as_array().map(Vec::len), Some(3));
    assert!(manifest["invocation"].is_null());
    assert_eq!(manifest["results"], json!([]));

    let first_manifest = fs::read(bundle.join("zryna-manifest-v2.json"))
        .expect("first M2 manifest bytes must remain readable");
    let expected_snapshot =
        include_str!("snapshots/control-flow-build-manifest-v2.json").replace("m2_snapshot", &stem);
    assert_eq!(first_manifest, expected_snapshot.as_bytes());
    fs::remove_dir_all(&bundle).expect("determinism fixture may remove its own first bundle");
    let repeated = command_output(
        &workspace,
        &[
            "build",
            &workspace.source_relative,
            "--profile",
            "control-flow-v1",
            "--target",
            "all",
            "--name",
            &stem,
            "--json",
        ],
    );
    assert_success(&repeated);
    assert_eq!(
        fs::read(bundle.join("zryna-manifest-v2.json"))
            .expect("repeated M2 manifest bytes must remain readable"),
        first_manifest
    );
}

#[test]
fn control_flow_portable_targets_share_typed_boolean_invocation() {
    let mut workspace = WorkspaceCase::control_flow();
    for target in ["javascript", "webassembly"] {
        let stem = workspace.stem(&format!("m2_bool_{target}"), "run");
        let output = command_output(
            &workspace,
            &[
                "run",
                &workspace.source_relative,
                "--profile",
                "control-flow-v1",
                "--target",
                target,
                "--name",
                &stem,
                "--export",
                "choose",
                "--arg=bool:true",
                "--arg=i32:21",
                "--json",
            ],
        );
        assert_success(&output);
        let response: Value = serde_json::from_slice(&output.stdout).expect("M2 run response");
        assert_eq!(
            response["results"],
            json!([{
                "target": target,
                "outcome": {"kind": "returned", "value": {"type": "i32", "value": 42}}
            }])
        );
        let manifest = read_json(&workspace.bundle(&stem, "run").join("zryna-manifest-v2.json"));
        assert_eq!(
            manifest["invocation"]["arguments"],
            json!([
                {"type": "bool", "value": true},
                {"type": "i32", "value": 21}
            ])
        );
        assert_eq!(manifest["results"][0]["outcome"]["value"], 42);
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
#[test]
fn control_flow_linux_all_run_reports_three_ordered_equal_results() {
    let mut workspace = WorkspaceCase::control_flow();
    let stem = workspace.stem("m2_bool_all", "run");
    let output = command_output(
        &workspace,
        &[
            "run",
            &workspace.source_relative,
            "--profile",
            "control-flow-v1",
            "--target",
            "all",
            "--name",
            &stem,
            "--export",
            "choose",
            "--arg=bool:true",
            "--arg=i32:21",
            "--json",
        ],
    );
    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("M2 all response");
    let results = response["results"].as_array().expect("ordered M2 results");
    assert_eq!(
        results.iter().map(|result| result["target"].as_str()).collect::<Vec<_>>(),
        [Some("javascript"), Some("webassembly"), Some("native")]
    );
    assert!(results.iter().all(|result| result["outcome"]["value"]["value"] == 42));
}

#[cfg(windows)]
#[test]
fn control_flow_windows_native_run_fails_closed_without_a_bundle() {
    for target in ["native", "all"] {
        let mut workspace = WorkspaceCase::control_flow();
        let stem = workspace.stem(&format!("m2_windows_{target}"), "run");
        let output = command_output(
            &workspace,
            &[
                "run",
                &workspace.source_relative,
                "--profile",
                "control-flow-v1",
                "--target",
                target,
                "--name",
                &stem,
                "--export",
                "choose",
                "--arg=bool:true",
                "--arg=i32:21",
                "--json",
            ],
        );
        assert_eq!(output.status.code(), Some(4));
        let response: Value =
            serde_json::from_slice(&output.stdout).expect("M2 rejection response");
        assert_eq!(response["ok"], false);
        assert!(response["diagnostics"].as_array().is_some_and(|diagnostics| {
            diagnostics.iter().any(|diagnostic| diagnostic["code"] == "ZRYNA-N4002")
        }));
        assert!(!workspace.bundle(&stem, "run").exists());
    }
}

#[test]
fn canonical_scalar_arguments_are_enforced_by_the_executable_parser() {
    for rejected in ["i32:+1", "i32:01", "i32:-0", "i32: 1", "bool:true"] {
        let output = zryna()
            .args([
                "run",
                "src/main.zry",
                "--target",
                "javascript",
                "--export",
                "add",
                "--arg",
                rejected,
                "--node",
            ])
            .arg(dummy_absolute_node())
            .output()
            .expect("invalid-argument command must start");
        assert_eq!(output.status.code(), Some(2), "{rejected} must be rejected");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("canonical"));
    }
}

#[test]
fn omitted_profile_keeps_clap_argument_errors_even_in_json_mode() {
    let output = zryna()
        .args([
            "run",
            "src/main.zry",
            "--target",
            "javascript",
            "--export",
            "add",
            "--arg=bool:true",
            "--json",
            "--node",
        ])
        .arg(dummy_absolute_node())
        .output()
        .expect("invalid legacy argument command must start");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid value 'bool:true'"));
    assert!(stderr.contains("For more information, try '--help'."));
    assert!(!stderr.contains("\"diagnostics\""));
}

#[test]
fn architecture_failure_precedes_output_creation() {
    let workspace = InvalidWorkspace::new();
    let output = zryna()
        .args([
            "build",
            "src/main.zry",
            "--target",
            "javascript",
            "--name",
            "must_not_exist",
            "--root",
        ])
        .arg(&workspace.root)
        .arg("--node")
        .arg(dummy_absolute_node())
        .output()
        .expect("architecture-failure command must start");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ZRYNA-A"));
    assert!(!workspace.root.join(".zryna").exists());
}

#[test]
fn javascript_build_and_run_publish_exact_bundles() {
    let mut case = WorkspaceCase::new();
    let build_stem = case.stem("javascript_build", "build");
    let build = command_output(
        &case,
        &["build", &case.source_relative, "--target", "javascript", "--name", &build_stem],
    );
    assert_success(&build);
    let build_bundle = case.bundle(&build_stem, "build");
    let manifest_path = build_bundle.join("zryna-manifest-v1.json");
    assert_eq!(
        build.stdout,
        format!(".zryna/out/{build_stem}.build/zryna-manifest-v1.json\n").as_bytes()
    );
    assert!(build_bundle.join(format!("javascript/{build_stem}.mjs")).is_file());
    assert!(!build_bundle.join("webassembly").exists());
    assert!(!build_bundle.join("native").exists());
    let manifest = read_json(&manifest_path);
    assert_eq!(manifest["command"], "build");
    assert_eq!(manifest["targets"], json!(["javascript"]));
    assert_eq!(manifest["results"], json!([]));

    let run_stem = case.stem("javascript_run", "run");
    let run = command_output(
        &case,
        &[
            "run",
            &case.source_relative,
            "--target",
            "javascript",
            "--name",
            &run_stem,
            "--export",
            "add",
            "--arg=i32:20",
            "--arg=i32:22",
        ],
    );
    assert_success(&run);
    assert_eq!(run.stdout, b"javascript: i32 42\n");
    let run_bundle = case.bundle(&run_stem, "run");
    assert!(run_bundle.join(format!("javascript/{run_stem}.mjs")).is_file());
    let manifest = read_json(&run_bundle.join("zryna-manifest-v1.json"));
    assert_eq!(
        manifest["invocation"]["arguments"],
        json!([
            {"type": "i32", "value": 20},
            {"type": "i32", "value": 22}
        ])
    );
    assert_eq!(manifest["results"][0]["outcome"]["value"], 42);
}

#[test]
fn webassembly_run_uses_canonical_i32_boundaries() {
    let mut case = WorkspaceCase::new();
    let stem = case.stem("webassembly_run", "run");
    let output = command_output(
        &case,
        &[
            "run",
            &case.source_relative,
            "--target",
            "webassembly",
            "--name",
            &stem,
            "--export",
            "add",
            "--arg=i32:2147483647",
            "--arg=i32:1",
        ],
    );
    assert_success(&output);
    assert_eq!(output.stdout, b"webassembly: i32 -2147483648\n");
    let bundle = case.bundle(&stem, "run");
    assert!(bundle.join(format!("webassembly/{stem}.wasm")).is_file());
    let manifest = read_json(&bundle.join("zryna-manifest-v1.json"));
    assert_eq!(manifest["targets"], json!(["webassembly"]));
    assert_eq!(manifest["results"][0]["outcome"]["value"], i32::MIN);
}

#[test]
fn webassembly_native_and_all_builds_publish_the_selected_artifacts() {
    let mut case = WorkspaceCase::new();
    for (target, expected) in [
        ("webassembly", &["webassembly"][..]),
        ("native", &["native"][..]),
        ("all", &["javascript", "webassembly", "native"][..]),
    ] {
        let stem = case.stem(&format!("{target}_build"), "build");
        let output = command_output(
            &case,
            &["build", &case.source_relative, "--target", target, "--name", &stem],
        );
        assert_success(&output);
        let bundle = case.bundle(&stem, "build");
        let manifest = read_json(&bundle.join("zryna-manifest-v1.json"));
        assert_eq!(manifest["targets"], json!(expected));
        assert_eq!(
            bundle.join(format!("javascript/{stem}.mjs")).exists(),
            expected.contains(&"javascript")
        );
        assert_eq!(
            bundle.join(format!("webassembly/{stem}.wasm")).exists(),
            expected.contains(&"webassembly")
        );
        assert_eq!(bundle.join(format!("native/{stem}.o")).exists(), expected.contains(&"native"));
    }
}

#[test]
fn invalid_source_uses_source_exit_and_publishes_nothing() {
    let mut case = WorkspaceCase::new();
    fs::write(case.root.join(&case.source_relative), "export function broken(: i32")
        .expect("invalid source fixture must be written");
    let stem = case.stem("invalid_source", "build");
    let output = command_output(
        &case,
        &["build", &case.source_relative, "--target", "javascript", "--name", &stem],
    );
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert!(!case.bundle(&stem, "build").exists());
}

#[cfg(unix)]
#[test]
fn target_execution_failure_removes_the_private_transaction() {
    let mut case = WorkspaceCase::new();
    let stem = case.stem("execution_failure", "run");
    let wrapper = compile_test_node(&case, "invalid-frame", "print!(\"bad\");");
    let child = zryna()
        .args([
            "run",
            &case.source_relative,
            "--target",
            "javascript",
            "--name",
            &stem,
            "--export",
            "add",
            "--arg=i32:20",
            "--arg=i32:22",
            "--root",
        ])
        .arg(&case.root)
        .arg("--node")
        .arg(&wrapper)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("execution-failure command must start");
    let transaction_prefix = format!(".zryna-transaction-{}-", child.id());
    let output = child.wait_with_output().expect("execution-failure command must finish");
    assert_eq!(
        output.status.code(),
        Some(5),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(!case.bundle(&stem, "run").exists());
    let leaked = fs::read_dir(case.root.join(".zryna/out"))
        .expect("output root must be readable")
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .any(|name| name.starts_with(&transaction_prefix));
    assert!(!leaked, "failed target execution must not leak a transaction");
}

#[cfg(unix)]
#[test]
fn unconfirmed_transaction_cleanup_uses_exit_six_and_no_bundle() {
    let mut case = WorkspaceCase::new();
    let stem = case.stem("cleanup_failure", "run");
    let marker_root = case.root.join(".zryna/cache").join(&case.unique);
    let token = format!("{}-cleanup-handshake", case.unique);
    let ready = marker_root.join(format!("{token}-ready"));
    let release = marker_root.join(format!("{token}-release"));
    let ready_literal = serde_json::to_string(ready.to_string_lossy().as_ref())
        .expect("ready path must serialize as a source literal");
    let release_literal = serde_json::to_string(release.to_string_lossy().as_ref())
        .expect("release path must serialize as a source literal");
    let wrapper_body = format!(
        r#"std::fs::create_dir({ready_literal}).expect("ready marker must be atomically published once");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {{
        match std::fs::metadata({release_literal}) {{
            Ok(metadata) if metadata.is_dir() => break,
            Ok(_) => std::process::exit(73),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {{}},
            Err(_) => std::process::exit(74),
        }}
        if std::time::Instant::now() >= deadline {{
            std::process::exit(75);
        }}
        std::thread::sleep(std::time::Duration::from_millis(10));
    }}
    print!("bad");"#,
    );
    let wrapper = compile_test_node(&case, "cleanup-handshake", &wrapper_body);
    let child = zryna()
        .args([
            "run",
            &case.source_relative,
            "--target",
            "javascript",
            "--name",
            &stem,
            "--export",
            "add",
            "--arg=i32:20",
            "--arg=i32:22",
            "--root",
        ])
        .arg(&case.root)
        .arg("--node")
        .arg(&wrapper)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cleanup-failure command must start");
    let mut child = TestChildGuard::new(child, release);
    let transaction_prefix = format!(".zryna-transaction-{}-", child.id());
    let output_root = case.root.join(".zryna/out");
    wait_for_test_marker(&ready, &mut child);
    let entries = fs::read_dir(&output_root)
        .expect("output root must be readable after runtime readiness")
        .collect::<Result<Vec<_>, _>>()
        .expect("every output-root entry must be readable");
    let stages = entries
        .into_iter()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&transaction_prefix))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    assert_eq!(stages.len(), 1, "runtime readiness must expose one private transaction");
    let stage = stages.into_iter().next().expect("one transaction was counted");
    let displaced = output_root.join(format!("{}.displaced", case.unique));
    child.own_path(stage.clone());
    child.own_path(displaced.clone());
    fs::rename(&stage, &displaced).expect("transaction stage must be displaced");
    fs::create_dir(&stage).expect("replacement transaction directory must be installed");

    let output = child.wait_with_output();
    assert_eq!(
        output.status.code(),
        Some(6),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(!case.bundle(&stem, "run").exists());
    assert!(stage.is_dir(), "replacement transaction must remain for test-owned cleanup");
    assert!(displaced.is_dir(), "displaced transaction must remain for test-owned cleanup");

    fs::remove_dir(&stage).expect("replacement transaction directory must be removed");
    fs::remove_dir_all(&displaced).expect("displaced transaction must be removed");
    let final_entries = fs::read_dir(&output_root)
        .expect("output root must remain readable")
        .collect::<Result<Vec<_>, _>>()
        .expect("every final output-root entry must be readable");
    let leaked = final_entries
        .into_iter()
        .any(|entry| entry.file_name().to_string_lossy().starts_with(&transaction_prefix));
    assert!(!leaked, "cleanup-failure test must leave no private transaction");
}

#[cfg(windows)]
#[test]
fn m1_windows_native_and_all_runs_are_rejected_without_a_bundle() {
    let mut case = WorkspaceCase::new();
    for target in ["native", "all"] {
        let stem = case.stem(&format!("{target}_unsupported_run"), "run");
        let output = command_output(
            &case,
            &[
                "run",
                &case.source_relative,
                "--target",
                target,
                "--name",
                &stem,
                "--export",
                "add",
                "--arg=i32:20",
                "--arg=i32:22",
            ],
        );
        assert_eq!(output.status.code(), Some(4));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("ZRYNA-N4002"));
        assert!(!case.bundle(&stem, "run").exists());
    }
}

#[test]
fn create_only_collision_preserves_the_complete_existing_bundle() {
    let mut case = WorkspaceCase::new();
    let stem = case.stem("collision", "build");
    let arguments =
        ["build", case.source_relative.as_str(), "--target", "javascript", "--name", stem.as_str()];
    let first = command_output(&case, &arguments);
    assert_success(&first);
    let bundle = case.bundle(&stem, "build");
    let manifest_path = bundle.join("zryna-manifest-v1.json");
    let artifact_path = bundle.join(format!("javascript/{stem}.mjs"));
    let original_manifest = fs::read(&manifest_path).expect("manifest must exist");
    let original_artifact = fs::read(&artifact_path).expect("artifact must exist");
    let unrelated = case.root.join(".zryna/out").join(format!("{}.sentinel", case.unique));
    fs::write(&unrelated, b"unrelated").expect("unrelated sentinel must be written");

    let second = command_output(&case, &arguments);
    assert_eq!(second.status.code(), Some(4));
    assert!(second.stdout.is_empty());
    assert!(String::from_utf8_lossy(&second.stderr).contains("ZRYNA-C1009"));
    assert_eq!(fs::read(manifest_path).expect("manifest must remain"), original_manifest);
    assert_eq!(fs::read(artifact_path).expect("artifact must remain"), original_artifact);
    assert_eq!(fs::read(&unrelated).expect("sentinel must remain"), b"unrelated");
    fs::remove_file(unrelated).expect("owned sentinel must be removed");
}

#[test]
fn json_failure_is_one_versioned_document_with_no_advertised_bundle() {
    let mut case = WorkspaceCase::new();
    let stem = case.stem("json_failure", "build");
    let missing = case.missing_entrypoint();
    let output = command_output(
        &case,
        &["build", &missing, "--target", "javascript", "--name", &stem, "--json"],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout must be one JSON document");
    let keys = response
        .as_object()
        .expect("response must be an object")
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        keys,
        BTreeSet::from(["command", "diagnostics", "manifest", "ok", "results", "version"])
    );
    assert_eq!(response["version"], 1);
    assert_eq!(response["ok"], false);
    assert_eq!(response["command"], "build");
    assert!(response["manifest"].is_null());
    assert_eq!(response["results"], json!([]));
    assert_eq!(response["diagnostics"][0]["code"], "ZRYNA-C1003");
    assert!(!case.bundle(&stem, "build").exists());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
#[test]
fn native_and_all_runs_publish_and_report_ordered_results() {
    let mut case = WorkspaceCase::new();
    let native_stem = case.stem("native_run", "run");
    let native = command_output(
        &case,
        &[
            "run",
            &case.source_relative,
            "--target",
            "native",
            "--name",
            &native_stem,
            "--export",
            "add",
            "--arg=i32:20",
            "--arg=i32:22",
        ],
    );
    assert_success(&native);
    assert_eq!(native.stdout, b"native: i32 42\n");
    assert!(case.bundle(&native_stem, "run").join(format!("native/{native_stem}.elf")).is_file());

    let all_stem = case.stem("all_run", "run");
    let all = command_output(
        &case,
        &[
            "run",
            &case.source_relative,
            "--target",
            "all",
            "--name",
            &all_stem,
            "--export",
            "add",
            "--arg=i32:2147483647",
            "--arg=i32:1",
        ],
    );
    assert_success(&all);
    assert_eq!(
        all.stdout,
        b"javascript: i32 -2147483648\nwebassembly: i32 -2147483648\nnative: i32 -2147483648\n"
    );
    let bundle = case.bundle(&all_stem, "run");
    assert!(bundle.join(format!("javascript/{all_stem}.mjs")).is_file());
    assert!(bundle.join(format!("webassembly/{all_stem}.wasm")).is_file());
    assert!(bundle.join(format!("native/{all_stem}.elf")).is_file());
    let manifest = read_json(&bundle.join("zryna-manifest-v1.json"));
    assert_eq!(manifest["targets"], json!(["javascript", "webassembly", "native"]));
    assert_eq!(manifest["results"].as_array().map(Vec::len), Some(3));
}

#[test]
fn new_project_is_deterministic_create_only_and_removable() {
    let first = ProjectCase::create("hello-project");
    let second = ProjectCase::create("hello-project");
    for relative in ["zryna.package.json", "zryna.lock.json", "src/main.zry"] {
        assert_eq!(
            fs::read(first.root.join(relative)).expect("first scaffold file"),
            fs::read(second.root.join(relative)).expect("second scaffold file"),
            "{relative} must be byte-deterministic"
        );
    }
    let original = fs::read(first.root.join("src/main.zry")).expect("original source");
    let collision =
        zryna().arg("new").arg(&first.root).output().expect("collision command must start");
    assert_eq!(collision.status.code(), Some(4));
    assert!(collision.stdout.is_empty());
    assert!(String::from_utf8_lossy(&collision.stderr).contains("ZRYNA-C2002"));
    assert_eq!(fs::read(first.root.join("src/main.zry")).expect("preserved source"), original);
    fs::remove_dir_all(&first.root).expect("standalone project must be removable as one tree");
    assert!(!first.root.exists());
}

#[test]
fn new_project_rejects_unsafe_names_paths_and_stages_without_cleanup_damage() {
    for name in ["con", "lpt1", "Uppercase", "bad name"] {
        let case = ProjectCase::empty(name);
        let output =
            zryna().arg("new").arg(&case.root).output().expect("invalid-name command must start");
        assert_eq!(output.status.code(), Some(2), "{name}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("ZRYNA-C2001"));
        assert!(!case.root.exists());
    }

    let case = ProjectCase::empty("safe-project");
    let traversal = case.parent.join("child").join("..").join("escaped-project");
    let output = zryna().arg("new").arg(traversal).output().expect("traversal command must start");
    assert_eq!(output.status.code(), Some(2));
    assert!(!case.parent.join("escaped-project").exists());

    fs::create_dir(&case.root).expect("empty destination collision");
    let empty_collision =
        zryna().arg("new").arg(&case.root).output().expect("empty collision command must start");
    assert_eq!(empty_collision.status.code(), Some(4));
    assert!(case.root.is_dir());
    fs::remove_dir(&case.root).expect("owned empty collision fixture");

    let stage = case.parent.join(".zryna-new-safe-project.pending");
    fs::create_dir(&stage).expect("pre-existing stage must be created");
    fs::write(stage.join("sentinel"), b"preserve").expect("stage sentinel must be written");
    let collision =
        zryna().arg("new").arg(&case.root).output().expect("stage collision command must start");
    assert_eq!(collision.status.code(), Some(4));
    assert_eq!(fs::read(stage.join("sentinel")).expect("stage sentinel"), b"preserve");
    assert!(!case.root.exists());
}

#[cfg(unix)]
#[test]
fn new_project_rejects_a_linked_parent_without_touching_its_target() {
    let target = ProjectCase::empty("target-parent");
    let linked = ProjectCase::empty("linked-parent");
    let parent_link = linked.parent.join("parent-link");
    std::os::unix::fs::symlink(&target.parent, &parent_link).expect("linked parent fixture");
    let destination = parent_link.join("linked-child");
    let rejected =
        zryna().arg("new").arg(&destination).output().expect("linked-parent command must start");
    assert_eq!(rejected.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("ZRYNA-C2001"));
    assert!(!target.parent.join("linked-child").exists());
}

#[test]
fn standalone_project_builds_and_runs_from_explicit_separate_roots() {
    let case = ProjectCase::create("external-project");
    let build = case.command(&[
        "build",
        "src/main.zry",
        "--target",
        "javascript",
        "--name",
        "external-build",
    ]);
    assert_success(&build);
    assert!(
        case.root.join(".zryna/out/external-build.build/javascript/external-build.mjs").is_file()
    );
    let run = case.command(&[
        "run",
        "src/main.zry",
        "--target",
        "webassembly",
        "--name",
        "external-run",
        "--export",
        "main",
    ]);
    assert_success(&run);
    assert_eq!(run.stdout, b"webassembly: i32 42\n");
    assert!(case.root.join(".zryna/out/external-run.run/webassembly/external-run.wasm").is_file());

    fs::write(case.root.join("undeclared.txt"), b"undeclared")
        .expect("undeclared fixture must be written");
    let rejected = case.command(&[
        "build",
        "src/main.zry",
        "--target",
        "javascript",
        "--name",
        "undeclared-build",
    ]);
    assert_eq!(rejected.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("ZRYNA-P4004"));
    assert!(!case.root.join(".zryna/out/undeclared-build.build").exists());
}

#[test]
fn exact_public_predecessor_projects_build_and_run_under_the_compatible_patch_compiler() {
    assert_eq!(env!("CARGO_PKG_VERSION"), "0.2.3");
    for version in ["0.2.1", "0.2.2"] {
        exercise_public_predecessor(version);
    }
}

fn exercise_public_predecessor(version: &str) {
    let case = ProjectCase::public_predecessor(version);
    let retained = ["zryna.package.json", "zryna.lock.json", "src/main.zry"].map(|relative| {
        (relative, fs::read(case.root.join(relative)).expect("legacy project file"))
    });
    let unrelated = case.parent.join("user-notes.txt");
    fs::write(&unrelated, b"retain project note\n").expect("external user file");

    for target in ["javascript", "webassembly"] {
        let build_name = format!("{target}-upgrade-build");
        assert_success(&case.command(&[
            "build",
            "src/main.zry",
            "--target",
            target,
            "--name",
            &build_name,
        ]));
        let run_name = format!("{target}-upgrade-run");
        let run = case.command(&[
            "run",
            "src/main.zry",
            "--target",
            target,
            "--name",
            &run_name,
            "--export",
            "main",
        ]);
        assert_success(&run);
        assert_eq!(run.stdout, format!("{target}: i32 42\n").as_bytes());
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    {
        assert_success(&case.command(&[
            "build",
            "src/main.zry",
            "--target",
            "native",
            "--name",
            "native-upgrade-build",
        ]));
        let run = case.command(&[
            "run",
            "src/main.zry",
            "--target",
            "native",
            "--name",
            "native-upgrade-run",
            "--export",
            "main",
        ]);
        assert_success(&run);
        assert_eq!(run.stdout, b"native: i32 42\n");
    }

    for (relative, bytes) in retained {
        assert_eq!(fs::read(case.root.join(relative)).expect("retained legacy file"), bytes);
    }
    assert_eq!(fs::read(unrelated).expect("retained external user file"), b"retain project note\n");
}

#[test]
fn standalone_project_cannot_compile_undeclared_state_source() {
    let case = ProjectCase::create("state-source-project");
    fs::create_dir_all(case.root.join(".zryna/cache")).expect("state cache directory");
    fs::write(
        case.root.join(".zryna/cache/undeclared.zry"),
        b"export function hidden(): i32 { return 7; }\n",
    )
    .expect("undeclared state source");
    let rejected = case.command(&[
        "build",
        ".zryna/cache/undeclared.zry",
        "--target",
        "javascript",
        "--name",
        "state-source-build",
    ]);
    assert_eq!(rejected.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("ZRYNA-P4004"));
    assert!(!case.root.join(".zryna/out/state-source-build.build").exists());
}

#[cfg(unix)]
#[test]
fn project_mutation_after_frontend_admission_stops_before_target_execution() {
    let case = ProjectCase::create("mutating-project");
    let marker = case.parent.join("target-executed");
    let node = compile_mutating_project_node(&case, &marker);
    let rejected = zryna()
        .args([
            "run",
            "src/main.zry",
            "--target",
            "javascript",
            "--name",
            "mutating-run",
            "--export",
            "main",
        ])
        .arg("--root")
        .arg(ProjectCase::compiler_root())
        .arg("--project-root")
        .arg(&case.root)
        .arg("--node")
        .arg(node)
        .output()
        .expect("mutating project command must start");
    assert_eq!(rejected.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("ZRYNA-P4004"));
    assert!(!marker.exists(), "target runtime must not execute after project mutation");
    assert!(!case.root.join(".zryna/out/mutating-run.run").exists());
}

#[cfg(unix)]
#[test]
fn standalone_project_root_link_is_rejected_without_output() {
    let real = ProjectCase::create("real-project");
    let linked = ProjectCase::empty("linked-project");
    std::os::unix::fs::symlink(&real.root, &linked.root).expect("project root link");
    let rejected = linked.command(&[
        "build",
        "src/main.zry",
        "--target",
        "javascript",
        "--name",
        "linked-build",
    ]);
    assert_eq!(rejected.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("ZRYNA-P4004"));
    assert!(!real.root.join(".zryna").exists());
}
