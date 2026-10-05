use std::{
    env, fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use super::{capture::CapturedToolingClosure, stage::ToolingStage};

static NEXT: AtomicU64 = AtomicU64::new(0);

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
    let stage = ToolingStage::create(&captured).expect("authenticated nine-file stage");
    let stage_path = stage.physical_path().to_path_buf();
    stage.revalidate().expect("authenticated stage inventory and identity");
    stage.abort().expect("explicit authenticated stage cleanup");
    assert!(!stage_path.exists(), "successful abort removes only the owned stage");
    for (from, to) in mappings {
        let destination = bootstrap.join(to);
        fs::write(&destination, b"untrusted substitute").expect("replace fixture");
        assert!(CapturedToolingClosure::capture_installed(&root).is_err(), "{to}");
        fs::copy(source.join(from), &destination).expect("restore fixture");
    }
    let moved = root.with_extension("relocated");
    fs::rename(&root, &moved).expect("relocate");
    CapturedToolingClosure::capture_installed(&moved).expect("relocated closure");
    fs::remove_dir_all(moved).expect("remove owned fixture");
}
