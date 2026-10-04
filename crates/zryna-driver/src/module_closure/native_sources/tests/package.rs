use serde_json::json;
use sha2::{Digest as _, Sha256};

use super::{Workspace, code, path};
use crate::{
    PackageLockMode, PackageResolutionRequest, capture_native_package_sources, resolve_package,
};

fn write_package(
    workspace: &Workspace,
    locator: &str,
    name: &str,
    files: &[(&str, &[u8])],
    dependencies: &serde_json::Value,
) {
    let inventory = files
        .iter()
        .map(|(path, bytes)| {
            json!({
                "path": path, "sha256": format!("{:x}", Sha256::digest(bytes)), "size": bytes.len(),
            })
        })
        .collect::<Vec<_>>();
    for (path, bytes) in files {
        workspace.write(&format!("{locator}/{path}"), bytes);
    }
    let manifest = json!({
        "compatibility": {"compiler": "0.1.0", "profile": "control-flow-v1", "targets": ["javascript", "webassembly"]},
        "dependencies": dependencies, "files": inventory, "format": "zryna.package.v1", "name": name,
        "source": {"kind": "local", "locator": locator, "revision": ""}, "version": "1.0.0",
    });
    let mut bytes = serde_json::to_vec(&manifest).expect("closed canonical manifest");
    bytes.push(b'\n');
    workspace.write(&format!("{locator}/zryna.package.json"), bytes);
}

fn request(workspace: &Workspace, mode: PackageLockMode) -> PackageResolutionRequest {
    PackageResolutionRequest {
        source_root: workspace.0.clone(),
        package: "packages/app".to_owned(),
        git_cache: None,
        mode,
    }
}

#[test]
fn native_package_capture_proves_exact_file_and_byte_limits_and_rejects_first_extra() {
    let workspace = Workspace::new("package-exact-limits");
    let sources = (0..16)
        .map(|index| {
            let mut text = if index + 1 < 16 {
                format!("import {{ value }} from './m{:02}.zry';\n//", index + 1)
            } else {
                "//".to_owned()
            };
            text.push_str(&"x".repeat(1024 - text.len()));
            (format!("m{index:02}.zry"), text.into_bytes())
        })
        .collect::<Vec<_>>();
    let files =
        sources.iter().map(|(name, text)| (name.as_str(), text.as_slice())).collect::<Vec<_>>();
    write_package(&workspace, "packages/app", "app", &files, &json!([]));
    let resolved = resolve_package(&request(&workspace, PackageLockMode::Update))
        .expect("exact package limits");
    let source = capture_native_package_sources(
        &request(&workspace, PackageLockMode::Frozen),
        resolved.graph().root(),
        path("m00.zry"),
    )
    .expect("exact-limit retained package source graph");
    assert_eq!(source.modules().len(), 16);
    source.verify_v3().expect("genuine complete exact-limit syntax");
    workspace.write("packages/app/extra.zry", " ");
    assert!(
        capture_native_package_sources(
            &request(&workspace, PackageLockMode::Frozen),
            resolved.graph().root(),
            path("m00.zry")
        )
        .is_err(),
        "first extra undeclared source file"
    );
    std::fs::remove_file(workspace.0.join("packages/app/extra.zry"))
        .expect("known fixture extra file");
    workspace.write("packages/app/m00.zry", "x".repeat(1025));
    assert!(
        capture_native_package_sources(
            &request(&workspace, PackageLockMode::Frozen),
            resolved.graph().root(),
            path("m00.zry")
        )
        .is_err(),
        "first extra package source byte"
    );
}

#[test]
fn native_package_capture_matches_workspace_graph_and_retains_exact_instance_and_lock() {
    let workspace = Workspace::new("package-local");
    let main =
        b"import { value } from './dep.zry';\nexport function main(): i32 { return value(); }\n";
    let dep = b"export function value(): i32 { return 7; }\n";
    write_package(
        &workspace,
        "packages/app",
        "app",
        &[("dep.zry", dep), ("main.zry", main)],
        &json!([]),
    );
    let resolved = resolve_package(&request(&workspace, PackageLockMode::Update))
        .expect("genuine package resolution");
    let package_id = resolved.graph().root();
    let package_source = capture_native_package_sources(
        &request(&workspace, PackageLockMode::Frozen),
        package_id,
        path("main.zry"),
    )
    .expect("frozen capture");
    let identity = package_source.package_identity().expect("exact source identity");
    assert_eq!(identity.0, package_id);
    assert_eq!(identity.2, resolved.graph().lock_sha256());
    let root = crate::WorkspaceSourceRoot::capture(&workspace.0.join("packages/app"))
        .expect("workspace root");
    let workspace_source = crate::capture_native_workspace_sources(&root, path("main.zry"))
        .expect("workspace capture");
    assert_eq!(package_source.modules(), workspace_source.modules());
    assert_eq!(package_source.graph_sha256_v3(), workspace_source.graph_sha256_v3());
    let original_map = package_source.sources().clone();
    let native = package_source.verify_v3().expect("package verification");
    assert!(native.closure().syntax().is_bound_to(&original_map));
    native.revalidate().expect("same package capability retained through syntax verification");
    let program = native.closure().lower_control_flow_v1().expect("genuine package-source program");
    assert!(
        !zryna_backend_javascript::emit_control_flow(&program)
            .expect("target dispatch")
            .source
            .is_empty()
    );
    native.revalidate().expect("same package source after dispatch");
}

#[test]
fn native_package_capture_selects_only_an_exact_admitted_dependency_instance() {
    let workspace = Workspace::new("package-dependency");
    write_package(
        &workspace,
        "packages/app",
        "app",
        &[("main.zry", b"export function main(): i32 { return 1; }\n")],
        &json!([{ "alias": "math", "name": "library", "source": {"kind": "local", "locator": "packages/library", "revision": ""}, "version": "1.0.0" }]),
    );
    write_package(
        &workspace,
        "packages/library",
        "library",
        &[("main.zry", b"export function value(): i32 { return 7; }\n")],
        &json!([]),
    );
    let resolved = resolve_package(&request(&workspace, PackageLockMode::Update)).expect("graph");
    let dependency = resolved
        .sources()
        .iter()
        .find(|source| source.package_id() != resolved.graph().root())
        .expect("admitted dependency")
        .package_id();
    let source = capture_native_package_sources(
        &request(&workspace, PackageLockMode::Frozen),
        dependency,
        path("main.zry"),
    )
    .expect("exact dependency inventory");
    assert_eq!(source.package_identity().expect("identity").0, dependency);
    source.verify_v3().expect("complete dependency syntax");
    assert_eq!(
        code(
            &capture_native_package_sources(
                &request(&workspace, PackageLockMode::Frozen),
                &"0".repeat(64),
                path("main.zry")
            )
            .err()
            .expect("unknown identity rejects")
        ),
        "ZRYNA-P4006"
    );
}

#[test]
fn native_package_capture_rejects_updates_escape_missing_inventory_aliases_and_wrong_hashes() {
    for (index, body) in [
        "import { value } from '../outside.zry';",
        "import { value } from './missing.zry';",
        "import { value } from 'math';",
    ]
    .iter()
    .enumerate()
    {
        let workspace = Workspace::new(&format!("package-escape-{index}"));
        write_package(
            &workspace,
            "packages/app",
            "app",
            &[("main.zry", body.as_bytes())],
            &json!([]),
        );
        let resolved =
            resolve_package(&request(&workspace, PackageLockMode::Update)).expect("graph");
        let package_id = resolved.graph().root();
        assert!(
            capture_native_package_sources(
                &request(&workspace, PackageLockMode::Frozen),
                package_id,
                path("main.zry")
            )
            .is_err()
        );
        assert_eq!(
            code(
                &capture_native_package_sources(
                    &request(&workspace, PackageLockMode::Update),
                    package_id,
                    path("main.zry")
                )
                .err()
                .expect("no lock mutation")
            ),
            "ZRYNA-P4010"
        );
    }
    let workspace = Workspace::new("package-wrong-hash");
    write_package(
        &workspace,
        "packages/app",
        "app",
        &[("main.zry", b"export function main(): i32 { return 1; }\n")],
        &json!([]),
    );
    let resolved = resolve_package(&request(&workspace, PackageLockMode::Update)).expect("graph");
    let before = std::fs::read(resolved.lock_path()).expect("original lock");
    workspace.write("packages/app/main.zry", "export function main(): i32 { return 9; }\n");
    assert_eq!(
        code(
            &capture_native_package_sources(
                &request(&workspace, PackageLockMode::Frozen),
                resolved.graph().root(),
                path("main.zry")
            )
            .err()
            .expect("wrong source hash rejects")
        ),
        "ZRYNA-P4004"
    );
    assert_eq!(std::fs::read(resolved.lock_path()).expect("preserved lock"), before);
}

#[cfg(unix)]
#[test]
fn native_package_snapshot_keeps_original_bytes_and_rejects_stale_retained_source() {
    let workspace = Workspace::new("package-stale");
    let original = b"export function main(): i32 { return 7; }\n";
    write_package(&workspace, "packages/app", "app", &[("main.zry", original)], &json!([]));
    let resolved = resolve_package(&request(&workspace, PackageLockMode::Update)).expect("graph");
    let source = capture_native_package_sources(
        &request(&workspace, PackageLockMode::Frozen),
        resolved.graph().root(),
        path("main.zry"),
    )
    .expect("snapshot");
    workspace.write("packages/app/main.zry", "export function main(): i32 { return 9; }\n");
    let id = source.sources().file_id(&path("main.zry")).expect("original id");
    assert_eq!(source.sources().source(id).expect("immutable source").text().as_bytes(), original);
    assert_eq!(code(&source.revalidate().expect_err("stale source")), "ZRYNA-P4004");
}
