use std::fs;

use super::{Workspace, code, path};
use crate::capture_native_workspace_sources;

#[test]
fn native_paths_reject_escape_ambient_forms_missing_sources_and_wrong_case() {
    for (index, specifier) in [
        "../escape.zry",
        "/escape.zry",
        "C:/escape.zry",
        "https://example.invalid/main.zry",
        "dependency",
        "./dep.ZRY",
        ".\\dep.zry",
        "./dep.zry?query",
        "./dep.zry#fragment",
        "./missing.zry",
        "./Dep.zry",
    ]
    .iter()
    .enumerate()
    {
        let workspace = Workspace::new(&format!("hostile-path-{index}"));
        workspace.write("main.zry", format!("import {{ value }} from '{specifier}';\n"));
        workspace.write("dep.zry", "export function value(): i32 { return 1; }\n");
        let root = workspace.root();
        assert!(capture_native_workspace_sources(&root, path("main.zry")).is_err(), "{specifier}");
    }
}

#[test]
fn native_sources_reject_cycles_duplicate_bindings_and_portable_collisions() {
    for (index, main, dep, expected) in [
        (0, "import { value } from './main.zry';", "", "ZRYNA-D3301"),
        (
            1,
            "import { value } from './dep.zry';",
            "import { other } from './main.zry';",
            "ZRYNA-D3301",
        ),
        (
            2,
            "import { value } from './dep.zry'; import { value } from './dep.zry';",
            "",
            "ZRYNA-D3006",
        ),
        (
            3,
            "import { value } from './dep.zry'; import { other } from './Dep.zry';",
            "",
            "ZRYNA-D3005",
        ),
    ] {
        let workspace = Workspace::new(&format!("hostile-graph-{index}"));
        workspace.write("main.zry", main);
        workspace.write("dep.zry", dep);
        let root = workspace.root();
        let error = capture_native_workspace_sources(&root, path("main.zry"))
            .err()
            .expect("invalid graph must reject before complete parsing");
        assert_eq!(code(&error), expected);
    }
}

#[test]
fn native_encoding_rejects_invalid_utf8_without_repair_or_partial_authority() {
    let workspace = Workspace::new("encoding");
    workspace.write("main.zry", [b'/', b'/', 0xff]);
    let root = workspace.root();
    assert_eq!(
        code(
            &capture_native_workspace_sources(&root, path("main.zry"))
                .err()
                .expect("invalid UTF-8 rejects")
        ),
        "ZRYNA-D3003"
    );
}

#[test]
fn native_full_parser_rejects_trailing_unsupported_source_after_discovery() {
    let workspace = Workspace::new("omitted-body");
    workspace.write("main.zry", "export function main(): i32 { return 1; }\nclass Hidden {}\n");
    let root = workspace.root();
    let source = capture_native_workspace_sources(&root, path("main.zry")).expect("source capture");
    assert!(source.verify_v3().is_err(), "unparsed declarations cannot become syntax authority");
}

#[cfg(unix)]
#[test]
fn native_no_follow_rejects_final_parent_and_root_links_and_hardlink_aliases() {
    use std::os::unix::fs::symlink;
    let workspace = Workspace::new("links");
    workspace.write("real/main.zry", "export function main(): i32 { return 1; }\n");
    symlink("real/main.zry", workspace.0.join("linked.zry")).expect("final link");
    symlink("real", workspace.0.join("linked")).expect("parent link");
    let root = workspace.root();
    for entry in ["linked.zry", "linked/main.zry"] {
        assert!(capture_native_workspace_sources(&root, path(entry)).is_err());
    }
    assert!(crate::WorkspaceSourceRoot::capture(&workspace.0.join("linked")).is_err());
    fs::hard_link(workspace.0.join("real/main.zry"), workspace.0.join("alias.zry")).expect("alias");
    assert!(capture_native_workspace_sources(&root, path("alias.zry")).is_err());
}

#[cfg(unix)]
#[test]
fn native_snapshot_rejects_concurrent_replacement_and_retains_original_bytes() {
    use std::sync::{Arc, Barrier};
    let workspace = Workspace::new("concurrent-substitution");
    let original = "export function main(): i32 { return 7; }\n";
    workspace.write("main.zry", original);
    workspace.write("replacement.zry", "export function main(): i32 { return 9; }\n");
    let root = workspace.root();
    let source =
        capture_native_workspace_sources(&root, path("main.zry")).expect("genuine snapshot");
    let barrier = Arc::new(Barrier::new(2));
    let writer_barrier = barrier.clone();
    let source_path = workspace.0.join("main.zry");
    let replacement = workspace.0.join("replacement.zry");
    let writer = std::thread::spawn(move || {
        writer_barrier.wait();
        fs::rename(replacement, source_path).expect("concurrent source binding replacement");
    });
    barrier.wait();
    writer.join().expect("independent writer completed");
    let id = source.sources().file_id(&path("main.zry")).expect("original file id");
    assert_eq!(source.sources().source(id).expect("immutable original source").text(), original);
    assert_eq!(code(&source.revalidate().expect_err("stale binding rejects")), "ZRYNA-D3004");
    assert!(source.verify_v3().is_err(), "stale source cannot enter final syntax verification");
}

#[cfg(unix)]
#[test]
fn native_snapshot_rejects_in_place_write_parent_swap_and_root_swap() {
    for mutation in 0..3 {
        let workspace = Workspace::new(&format!("stale-handles-{mutation}"));
        workspace.write("project/src/main.zry", "export function main(): i32 { return 7; }\n");
        let root = crate::WorkspaceSourceRoot::capture(&workspace.0.join("project")).expect("root");
        let source =
            capture_native_workspace_sources(&root, path("src/main.zry")).expect("capture");
        match mutation {
            0 => fs::write(
                workspace.0.join("project/src/main.zry"),
                "export function main(): i32 { return 9; }\n",
            )
            .expect("same-length in-place mutation"),
            1 => {
                fs::rename(workspace.0.join("project/src"), workspace.0.join("project/old-src"))
                    .expect("parent move");
                fs::create_dir(workspace.0.join("project/src")).expect("foreign parent");
            }
            _ => {
                fs::rename(workspace.0.join("project"), workspace.0.join("old-project"))
                    .expect("root move");
                fs::create_dir(workspace.0.join("project")).expect("foreign root");
            }
        }
        assert_eq!(code(&source.revalidate().expect_err("stale retained owner")), "ZRYNA-D3004");
    }
}

#[cfg(windows)]
#[test]
fn native_windows_retained_handles_block_concurrent_write_delete_and_parent_rename() {
    let workspace = Workspace::new("windows-sharing");
    workspace.write("src/main.zry", "export function main(): i32 { return 7; }\n");
    let root = workspace.root();
    let source = capture_native_workspace_sources(&root, path("src/main.zry")).expect("capture");
    let filename = workspace.0.join("src/main.zry");
    let parent = workspace.0.join("src");
    let moved = workspace.0.join("moved");
    let writer = std::thread::spawn(move || {
        assert!(fs::write(&filename, "changed").is_err());
        assert!(fs::remove_file(&filename).is_err());
        assert!(fs::rename(parent, moved).is_err());
    });
    writer.join().expect("independent concurrent writer");
    source.revalidate().expect("sharing protection preserves source authority");
    source.verify_v3().expect("unchanged source verifies");
}

#[cfg(windows)]
#[test]
fn native_windows_rejects_junction_source_traversal() {
    let workspace = Workspace::new("windows-junction");
    workspace.write("real/main.zry", "export function main(): i32 { return 7; }\n");
    let junction = workspace.0.join("linked");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(workspace.0.join("real"))
        .status()
        .expect("create junction");
    assert!(status.success());
    let root = workspace.root();
    assert!(capture_native_workspace_sources(&root, path("linked/main.zry")).is_err());
    drop(root);
    fs::remove_dir(junction).expect("remove known fixture junction");
}
