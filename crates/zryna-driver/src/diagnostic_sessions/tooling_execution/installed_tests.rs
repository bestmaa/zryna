use std::{
    env, fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use super::{capture::CapturedToolingClosure, stage::ToolingStage};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) fn legacy_worker() -> Vec<u8> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new("git")
        .current_dir(source)
        .args(["show", "v0.2.3:adapters/typescript-6/src/worker-v4.mjs"])
        .output()
        .expect("read historical repository source");
    assert!(output.status.success(), "historical v0.2.3 source must be available");
    assert_eq!(output.stdout.len(), 73_118);
    output.stdout
}

#[test]
fn installed_legacy_v4_pin_preserves_the_exact_nine_file_stage() {
    let root = env::temp_dir().join(format!(
        "zryna-legacy-installed-tooling-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bootstrap = root.join("lib/zryna/bootstrap");
    for name in ["worker.mjs", "worker-v3.mjs", "limits-v3.mjs", "worker-v4.mjs", "limits-v4.mjs"] {
        fs::create_dir_all(&bootstrap).expect("bootstrap fixture");
        fs::copy(source.join("adapters/typescript-6/src").join(name), bootstrap.join(name))
            .expect("worker fixture");
    }
    for (package, installed) in [
        ("@typescript+typescript6@6.0.2/node_modules/@typescript/typescript6", "typescript6"),
        ("typescript@6.0.3/node_modules/typescript", "old"),
    ] {
        for name in ["package.json", "lib/typescript.js"] {
            let destination = bootstrap.join("node_modules/@typescript").join(installed).join(name);
            fs::create_dir_all(destination.parent().expect("dependency parent"))
                .expect("directories");
            fs::copy(source.join("node_modules/.pnpm").join(package).join(name), destination)
                .expect("dependency fixture");
        }
    }
    let legacy = legacy_worker();
    fs::write(bootstrap.join("worker-v4.mjs"), &legacy).expect("historical worker");
    let captured =
        CapturedToolingClosure::capture_installed(&root).expect("historical installed closure");
    let stage = ToolingStage::create(&captured).expect("historical private stage");
    stage.revalidate().expect("exact historical stage");
    assert!(!stage.working_directory().join("v4").exists());
    assert_eq!(fs::read(stage.worker_v4()).expect("historical bytes"), legacy);
    assert_eq!(stage_file_count(stage.working_directory()), 9);
    let mut child = std::process::Command::new("node")
        .arg(stage.worker_v4())
        .current_dir(stage.working_directory())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("historical staged worker");
    std::io::Write::write_all(
        &mut child.stdin.take().expect("worker input"),
        b"{\"id\":1,\"method\":\"handshake\"}\n",
    )
    .expect("handshake");
    let output = child.wait_with_output().expect("worker output");
    assert!(output.status.success());
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("handshake JSON");
    assert_eq!(response["result"]["protocol_version"], 4);
    stage.revalidate().expect("execution preserves historical stage");
    let path = stage.physical_path().to_path_buf();
    drop(stage);
    assert!(!path.exists(), "historical stage cleanup");
    let stage = ToolingStage::create(&captured).expect("historical stage for unknown-entry check");
    let path = stage.physical_path().to_path_buf();
    fs::create_dir(path.join("v4")).expect("unknown modular directory");
    fs::write(path.join("v4/retain"), b"foreign").expect("foreign entry");
    assert!(stage.revalidate().is_err());
    drop(stage);
    assert!(path.join("v4/retain").exists(), "unknown entries must be retained");
    fs::remove_dir_all(path).expect("test-owned unknown-entry cleanup");

    let mut wrong = legacy.clone();
    wrong.push(b'\n');
    fs::write(bootstrap.join("worker-v4.mjs"), wrong).expect("wrong historical hash");
    assert!(CapturedToolingClosure::capture_installed(&root).is_err());
    fs::write(bootstrap.join("worker-v4.mjs"), &legacy).expect("restore historical pin");
    fs::create_dir(bootstrap.join("v4")).expect("mixed empty tree");
    assert!(CapturedToolingClosure::capture_installed(&root).is_err());
    fs::create_dir(bootstrap.join("v4/boundary")).expect("partial module tree");
    fs::copy(
        source.join("adapters/typescript-6/src/v4/boundary/transport.mjs"),
        bootstrap.join("v4/boundary/transport.mjs"),
    )
    .expect("mixed module");
    assert!(CapturedToolingClosure::capture_installed(&root).is_err());
    fs::remove_dir_all(bootstrap.join("v4")).expect("remove mixed module tree");
    fs::write(bootstrap.join("v4"), b"unknown form").expect("non-directory module form");
    assert!(CapturedToolingClosure::capture_installed(&root).is_err());
    fs::remove_file(bootstrap.join("v4")).expect("remove unknown form");
    fs::copy(
        source.join("adapters/typescript-6/src/worker-v4.mjs"),
        bootstrap.join("worker-v4.mjs"),
    )
    .expect("modular entrypoint without modules");
    assert!(CapturedToolingClosure::capture_installed(&root).is_err(), "no legacy fallback");
    fs::write(bootstrap.join("worker-v4.mjs"), &legacy).expect("restore historical worker");
    let limits_path = bootstrap.join("limits-v4.mjs");
    let limits = fs::read(&limits_path).expect("historical limit pin");
    fs::write(&limits_path, b"wrong limits").expect("substituted historical limits");
    assert!(CapturedToolingClosure::capture_installed(&root).is_err());
    fs::write(limits_path, limits).expect("restore limit pin");
    CapturedToolingClosure::capture_installed(&root).expect("restored historical closure");
    let mut malformed = CapturedToolingClosure::capture_installed(&root).expect("legacy fixture");
    malformed.v4 = super::capture::CapturedV4::Modular(Vec::new());
    assert!(ToolingStage::create(&malformed).is_err(), "empty modular form never selects legacy");
    fs::remove_dir_all(root).expect("remove fixture");
}

fn stage_file_count(path: &Path) -> usize {
    fs::read_dir(path)
        .expect("stage directory")
        .map(|entry| {
            let entry = entry.expect("stage entry");
            if entry.file_type().expect("stage entry kind").is_dir() {
                stage_file_count(&entry.path())
            } else {
                1
            }
        })
        .sum()
}

#[test]
fn installed_closure_rejects_missing_changed_worker_and_dependency_bytes() {
    let root = env::temp_dir().join(format!(
        "zryna-installed-tooling-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).expect("isolated fixture");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bootstrap = root.join("lib/zryna/bootstrap");
    let mappings = [
        ("adapters/typescript-6/src/worker.mjs", "worker.mjs"),
        ("adapters/typescript-6/src/worker-v3.mjs", "worker-v3.mjs"),
        ("adapters/typescript-6/src/limits-v3.mjs", "limits-v3.mjs"),
        ("adapters/typescript-6/src/worker-v4.mjs", "worker-v4.mjs"),
        ("adapters/typescript-6/src/limits-v4.mjs", "limits-v4.mjs"),
        (
            "adapters/typescript-6/src/v4/boundary/configuration.mjs",
            "v4/boundary/configuration.mjs",
        ),
        ("adapters/typescript-6/src/v4/boundary/dispatch.mjs", "v4/boundary/dispatch.mjs"),
        ("adapters/typescript-6/src/v4/boundary/errors.mjs", "v4/boundary/errors.mjs"),
        ("adapters/typescript-6/src/v4/boundary/request.mjs", "v4/boundary/request.mjs"),
        ("adapters/typescript-6/src/v4/boundary/transport.mjs", "v4/boundary/transport.mjs"),
        ("adapters/typescript-6/src/v4/syntax/constructions.mjs", "v4/syntax/constructions.mjs"),
        (
            "adapters/typescript-6/src/v4/syntax/data-declarations.mjs",
            "v4/syntax/data-declarations.mjs",
        ),
        ("adapters/typescript-6/src/v4/syntax/diagnostics.mjs", "v4/syntax/diagnostics.mjs"),
        (
            "adapters/typescript-6/src/v4/syntax/expression-arena.mjs",
            "v4/syntax/expression-arena.mjs",
        ),
        ("adapters/typescript-6/src/v4/syntax/expressions.mjs", "v4/syntax/expressions.mjs"),
        ("adapters/typescript-6/src/v4/syntax/functions.mjs", "v4/syntax/functions.mjs"),
        ("adapters/typescript-6/src/v4/syntax/imports.mjs", "v4/syntax/imports.mjs"),
        ("adapters/typescript-6/src/v4/syntax/matches.mjs", "v4/syntax/matches.mjs"),
        ("adapters/typescript-6/src/v4/syntax/names.mjs", "v4/syntax/names.mjs"),
        ("adapters/typescript-6/src/v4/syntax/source.mjs", "v4/syntax/source.mjs"),
        ("adapters/typescript-6/src/v4/syntax/spans.mjs", "v4/syntax/spans.mjs"),
        ("adapters/typescript-6/src/v4/syntax/statements.mjs", "v4/syntax/statements.mjs"),
        ("adapters/typescript-6/src/v4/syntax/tokens.mjs", "v4/syntax/tokens.mjs"),
        ("adapters/typescript-6/src/v4/syntax/types.mjs", "v4/syntax/types.mjs"),
        (
            "node_modules/.pnpm/@typescript+typescript6@6.0.2/node_modules/@typescript/typescript6/package.json",
            "node_modules/@typescript/typescript6/package.json",
        ),
        (
            "node_modules/.pnpm/@typescript+typescript6@6.0.2/node_modules/@typescript/typescript6/lib/typescript.js",
            "node_modules/@typescript/typescript6/lib/typescript.js",
        ),
        (
            "node_modules/.pnpm/typescript@6.0.3/node_modules/typescript/package.json",
            "node_modules/@typescript/old/package.json",
        ),
        (
            "node_modules/.pnpm/typescript@6.0.3/node_modules/typescript/lib/typescript.js",
            "node_modules/@typescript/old/lib/typescript.js",
        ),
    ];
    assert!(CapturedToolingClosure::capture_installed(&root).is_err());
    for (from, to) in mappings {
        let destination = bootstrap.join(to);
        fs::create_dir_all(destination.parent().expect("parent")).expect("directories");
        fs::copy(source.join(from), destination).expect("pinned source");
    }
    let captured =
        CapturedToolingClosure::capture_installed(&root).expect("complete installed closure");
    let stage = ToolingStage::create(&captured).expect("modular installed stage");
    assert_eq!(stage_file_count(stage.working_directory()), 28);
    assert!(stage.working_directory().join("v4/boundary").is_dir());
    assert!(stage.working_directory().join("v4/syntax").is_dir());
    stage.revalidate().expect("exact modular stage");
    drop(stage);
    for (from, to) in mappings {
        let destination = bootstrap.join(to);
        fs::write(&destination, b"untrusted substitute").expect("replace fixture");
        assert!(CapturedToolingClosure::capture_installed(&root).is_err(), "{to}");
        fs::copy(source.join(from), &destination).expect("restore fixture");
    }
    for (_, to) in mappings {
        let destination = bootstrap.join(to);
        let bytes = fs::read(&destination).expect("original fixture bytes");
        fs::remove_file(&destination).expect("remove fixture member");
        assert!(CapturedToolingClosure::capture_installed(&root).is_err(), "missing {to}");
        fs::write(&destination, bytes).expect("restore missing fixture member");
    }
    let moved = root.with_extension("relocated");
    fs::rename(&root, &moved).expect("relocate");
    CapturedToolingClosure::capture_installed(&moved).expect("relocated closure");
    fs::remove_dir_all(moved).expect("remove owned fixture");
}
