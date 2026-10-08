use std::{
    env, fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use super::capture::CapturedToolingClosure;

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
    CapturedToolingClosure::capture_installed(&root).expect("complete installed closure");
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
