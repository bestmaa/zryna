use std::{
    env,
    ffi::OsString,
    fs,
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use zryna_frontend::{
    FrontendCapabilities, ProviderExpectation, WorkerFrontend, WorkerLimits, WorkerSpec, syntax_v2,
};
use zryna_source::{MAX_SOURCE_FILE_BYTES, SourceFileInput, SourceMap};

use super::{CapturedSource, prepare_source};
use crate::{
    server_lifecycle::{Input, Limits},
    server_runtime::{Envelope, Error, Observation, Server},
};

pub(crate) fn production_frontend() -> WorkerFrontend {
    WorkerFrontend::new(production_spec())
}

fn production_spec() -> WorkerSpec {
    let adapter = node_path(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../adapters/typescript-6")
            .canonicalize()
            .expect("real production adapter directory"),
    );
    let node = ["ZRYNA_TEST_NODE", "NODE"]
        .into_iter()
        .find_map(env::var_os)
        .map_or_else(
            || {
                let executable = if cfg!(windows) { "node.exe" } else { "node" };
                env::split_paths(&env::var_os("PATH").expect("test PATH"))
                    .map(|directory| directory.join(executable))
                    .find(|candidate| candidate.is_file())
                    .expect("Node.js is required; this production-provider test cannot be skipped")
            },
            PathBuf::from,
        )
        .canonicalize()
        .expect("real Node.js executable");
    let expected = ProviderExpectation::new(
        "typescript-6",
        "6.0.3",
        syntax_v2::PROTOCOL_VERSION,
        FrontendCapabilities { module_resolution: false, semantic_diagnostics: false },
    )
    .expect("normal exact production expectation");
    WorkerSpec::new(
        node_path(node),
        vec![OsString::from("src/worker.mjs")],
        adapter,
        expected,
        WorkerLimits::default(),
    )
    .expect("normal production worker configuration")
}

// Canonical paths remain the fixture's identity, but Node's Windows invocation uses ordinary
// drive/UNC paths, matching the existing runtime's node_compatible_path boundary.
fn node_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let encoded = path.as_os_str().encode_wide().collect::<Vec<_>>();
        let prefix = [92_u16, 92, 63, 92];
        let Some(remainder) = encoded.strip_prefix(&prefix) else {
            return path;
        };
        let ordinary = if let Some(unc) = remainder.strip_prefix(&[85_u16, 78, 67, 92]) {
            [92_u16, 92].into_iter().chain(unc.iter().copied()).collect()
        } else {
            remainder.to_vec()
        };
        PathBuf::from(OsString::from_wide(&ordinary))
    }
    #[cfg(not(windows))]
    path
}

#[cfg(windows)]
#[test]
fn production_worker_uses_node_compatible_paths_and_authenticates_real_source() {
    use std::path::{Component, Prefix};
    let spec = production_spec();
    for (canonical, ordinary) in [
        (r"\\?\C:\node\node.exe", r"C:\node\node.exe"),
        (r"\\?\UNC\server\share\adapter", r"\\server\share\adapter"),
    ] {
        assert_eq!(node_path(PathBuf::from(canonical)), PathBuf::from(ordinary));
    }
    for path in [spec.executable(), spec.current_dir()] {
        assert!(path.is_absolute());
        assert!(
            matches!(
                path.components().next(),
                Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::UNC(_, _))
            ),
            "Node invocation cannot retain a Windows verbatim namespace: {path:?}"
        );
    }
    let sources = capture("export function status(): i32 { return 201; }");
    WorkerFrontend::new(spec)
        .analyze_verified(&sources.sources)
        .expect("real Windows worker handshake, response and source authentication");
}

fn envelope() -> Envelope {
    Envelope {
        requests: Limits {
            requests: 1,
            request_bytes: 32,
            response_bytes: 1,
            buffer_bytes: 128,
            timeout: Duration::from_secs(5),
        },
        memory_bytes: 65_536,
        fuel: 100_000,
        resources: 4,
        callbacks: 8,
    }
}

fn capture(text: &str) -> CapturedSource {
    CapturedSource::from_bytes("status.zry", text.as_bytes().to_vec()).expect("source snapshot")
}

fn clean(observation: &Observation) {
    assert_eq!(observation.live(), (0, 0));
    assert_eq!(observation.input_copies.load(Ordering::SeqCst), 0);
    assert_eq!(observation.input_copy_bytes.load(Ordering::SeqCst), 0);
    assert_eq!(
        observation.stores_created.load(Ordering::SeqCst),
        observation.stores_destroyed.load(Ordering::SeqCst)
    );
    assert_eq!(
        observation.created.load(Ordering::SeqCst),
        observation.destroyed.load(Ordering::SeqCst)
    );
}

struct SourceDirectory(PathBuf);
impl Drop for SourceDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn captured_source_executes_after_original_file_is_changed_and_deleted() {
    let directory = SourceDirectory(env::temp_dir().join(format!(
        "zryna-server-source-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).expect("fixture timestamp").as_nanos()
    )));
    fs::create_dir(&directory.0).expect("isolated source fixture");
    let path = directory.0.join("status.zry");
    fs::write(&path, "export function status(): i32 { return 199 + 3; }")
        .expect("original source bytes");
    let captured = CapturedSource::from_bytes("status.zry", fs::read(&path).expect("capture once"))
        .expect("captured source");
    let observation = Arc::new(Observation::default());
    let prepared = prepare_source(
        &production_frontend(),
        captured,
        "status",
        envelope(),
        Arc::clone(&observation),
    )
    .expect("real source, production frontend, verified IR and component");
    fs::write(&path, "export function status(): i32 { return 600; }")
        .expect("mutate source after preparation");
    fs::remove_file(&path).expect("remove original file before serving");
    fs::remove_dir(&directory.0).expect("source fixture removed");
    let server = Server::start(prepared).expect("source binding still validates");
    let input = Input {
        method: "GET",
        path: "/source",
        body: b"",
        deadline: Instant::now() + Duration::from_secs(4),
    };
    assert_eq!(server.admit(&input).expect("admit").wait(), Ok((202, vec![])));
    assert_eq!(server.usage(), Ok((0, 0)));
    assert_eq!(observation.clock_reads.load(Ordering::SeqCst), 0);
    assert_eq!(observation.denials.load(Ordering::SeqCst), 0);
    server.shutdown().expect("actual Store joined");
    clean(&observation);
}

#[test]
fn production_source_effects_and_wrong_exports_fail_before_engine_creation() {
    let frontend = production_frontend();
    for (text, export) in [
        ("export function status(): i32 { return Date.now(); }", "status"),
        ("export function status(): i32 { return process.env.STATUS; }", "status"),
        ("export function status(): i32 { return fetch('/remote'); }", "status"),
        ("export function status(): i32 { return Math.random(); }", "status"),
        (
            "function unused(): i32 { return Math.random(); } export function status(): i32 { return 200; }",
            "status",
        ),
        ("export function status(): i32 { return 200; }", "missing"),
        ("export function status(value: i32): i32 { return value; }", "status"),
        ("export function status(): bool { return true; }", "status"),
    ] {
        let observation = Arc::new(Observation::default());
        assert!(
            matches!(
                prepare_source(
                    &frontend,
                    capture(text),
                    export,
                    envelope(),
                    Arc::clone(&observation)
                ),
                Err(Error::Artifact)
            ),
            "rejected production source: {text}"
        );
        assert_eq!(observation.engines_constructed.load(Ordering::SeqCst), 0);
        assert_eq!(observation.stores_created.load(Ordering::SeqCst), 0);
        assert_eq!(observation.clock_reads.load(Ordering::SeqCst), 0);
        clean(&observation);
    }
}

#[test]
fn production_snapshot_cannot_lower_against_another_source_identity() {
    let captured = capture("export function status(): i32 { return 201; }");
    let snapshot = production_frontend()
        .analyze_verified(&captured.sources)
        .expect("production source-bound snapshot");
    let other = SourceMap::build(vec![SourceFileInput {
        path: "status.zry".into(),
        text: "export function status(): i32 { return 201; }".into(),
    }])
    .expect("identical bytes with independently allocated source authority");
    assert_ne!(captured.sources.identity(), other.identity());
    let errors = crate::lower_verified_syntax(&snapshot, &other)
        .expect_err("matching bytes cannot replace the authenticated source object");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "ZRYNA-D1001");
}

#[test]
fn capture_rejects_invalid_utf8_oversized_bytes_and_non_normalized_paths() {
    for (path, bytes) in [
        ("status.zry", vec![0xff]),
        ("status.zry", vec![b'x'; MAX_SOURCE_FILE_BYTES + 1]),
        ("../status.zry", b"export function status(): i32 { return 200; }".to_vec()),
    ] {
        assert!(matches!(CapturedSource::from_bytes(path, bytes), Err(Error::Artifact)));
    }
}
