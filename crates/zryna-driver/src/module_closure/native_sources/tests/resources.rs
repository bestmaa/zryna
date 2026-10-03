use std::fmt::Write as _;

use super::{Workspace, code, path};
use crate::{MAX_MODULE_FILES, MAX_MODULE_SOURCE_BYTES, capture_native_workspace_sources};
use zryna_source::MAX_SOURCE_FILE_BYTES;

#[test]
#[ignore = "proportional production named-binding edge boundary"]
fn native_snapshot_exact_binding_edge_count_and_first_extra_are_enforced() {
    for extra in [0, 1] {
        let workspace = Workspace::new(&format!("edge-limit-{extra}"));
        for module in 0..4 {
            let mut text = String::new();
            for declaration in 0..64 {
                let names = (0..256)
                    .map(|binding| format!("v{}", declaration * 256 + binding))
                    .collect::<Vec<_>>()
                    .join(",");
                writeln!(text, "import {{ {names} }} from './m{}.zry';", module + 1)
                    .expect("resource source text");
            }
            if module == 0 && extra == 1 {
                text.push_str("import { extra } from './m1.zry';\n");
            }
            workspace.write(&format!("m{module}.zry"), text);
        }
        workspace.write("m4.zry", "");
        let root = workspace.root();
        let result = capture_native_workspace_sources(&root, path("m0.zry"));
        if extra == 0 {
            let source = result.expect("genuine exact binding-edge limit");
            assert_eq!(source.edges.len(), crate::MAX_MODULE_IMPORT_EDGES);
            source.verify_v3().expect("complete exact-limit source syntax");
        } else {
            // The unchanged syntax import-binding limit shares the same exact ceiling and
            // rejects the first extra before graph allocation.
            assert_eq!(code(&result.err().expect("first extra named edge")), "ZRYNA-F1002");
        }
    }
}

#[test]
#[ignore = "proportional production source-file boundary"]
fn native_snapshot_exact_source_file_bytes_and_first_extra_are_enforced() {
    for extra in [0, 1] {
        let workspace = Workspace::new(&format!("source-limit-{extra}"));
        let mut text = b"//".to_vec();
        text.resize(MAX_SOURCE_FILE_BYTES + extra, b'x');
        workspace.write("main.zry", text);
        let root = workspace.root();
        let result = capture_native_workspace_sources(&root, path("main.zry"));
        if extra == 0 {
            let snapshot = result.expect("exact source-file limit");
            assert_eq!(
                snapshot
                    .sources()
                    .source(snapshot.sources().file_id(&path("main.zry")).expect("id"))
                    .expect("source")
                    .text()
                    .len(),
                MAX_SOURCE_FILE_BYTES
            );
            snapshot
                .verify_v3()
                .expect("genuine empty executable-syntax candidate at exact byte limit");
        } else {
            assert_eq!(code(&result.err().expect("first extra source byte")), "ZRYNA-D3201");
        }
    }
}

#[test]
#[ignore = "proportional production aggregate-source boundary"]
fn native_snapshot_exact_aggregate_bytes_and_first_extra_are_enforced() {
    for extra in [0, 1] {
        let workspace = Workspace::new(&format!("aggregate-limit-{extra}"));
        for index in 0..4 {
            let mut text = if index < 3 {
                format!("import {{ value }} from './module{}.zry';\n//", index + 1).into_bytes()
            } else {
                b"//".to_vec()
            };
            text.resize(MAX_SOURCE_FILE_BYTES, b'x');
            workspace.write(&format!("module{index}.zry"), text);
        }
        if extra == 1 {
            // Add one reachable source byte without making any individual file oversized.
            let mut first =
                b"import { value } from './module1.zry';\nimport { extra } from './extra.zry';\n//"
                    .to_vec();
            first.resize(MAX_SOURCE_FILE_BYTES, b'x');
            workspace.write("module0.zry", first);
            workspace.write("extra.zry", " ");
        }
        let root = workspace.root();
        let result = capture_native_workspace_sources(&root, path("module0.zry"));
        if extra == 0 {
            let snapshot = result.expect("exact aggregate source limit");
            let bytes: usize = snapshot
                .modules()
                .iter()
                .map(|record| {
                    snapshot
                        .sources()
                        .source(snapshot.sources().file_id(record.path()).expect("id"))
                        .expect("source")
                        .text()
                        .len()
                })
                .sum();
            assert_eq!(bytes, MAX_MODULE_SOURCE_BYTES);
            snapshot.verify_v3().expect("genuine exact-limit source graph syntax");
        } else {
            assert_eq!(code(&result.err().expect("first aggregate extra byte")), "ZRYNA-D3201");
        }
    }
}

#[test]
#[ignore = "proportional production retained file-count boundary"]
fn native_snapshot_exact_file_count_and_first_extra_are_enforced() {
    for extra in [0, 1] {
        let workspace = Workspace::new(&format!("graph-limit-{extra}"));
        let count = MAX_MODULE_FILES + extra;
        for index in 0..count {
            let text = if index + 1 < count {
                format!("import {{ value }} from './m{:04}.zry';\n", index + 1)
            } else {
                String::new()
            };
            workspace.write(&format!("m{index:04}.zry"), text);
        }
        let root = workspace.root();
        let result = capture_native_workspace_sources(&root, path("m0000.zry"));
        if extra == 0 {
            let snapshot = result.expect("exact retained file graph limit");
            assert_eq!(snapshot.modules().len(), MAX_MODULE_FILES);
            snapshot.verify_v3().expect("complete genuine exact-file-limit syntax");
        } else {
            assert_eq!(code(&result.err().expect("first extra reachable file")), "ZRYNA-D3201");
        }
    }
}
