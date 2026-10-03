//! Consume only #413's exported snapshot API; retain source authority through dispatch.

use std::{error::Error, fs, path::Path};

use zryna_backend_javascript as js;
use zryna_backend_webassembly as wasm;
use zryna_driver::{WorkspaceSourceRoot, capture_native_workspace_sources};
use zryna_semantics::{control_flow_v1 as m2, data_ownership_v1 as m3};
use zryna_source::NormalizedSourcePath;

use crate::handshake;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn path(value: &str) -> NormalizedSourcePath {
    NormalizedSourcePath::new(value).expect("portable fixture path")
}

pub fn smoke(cwd: &Path) -> Result<Vec<String>> {
    let mut passed = Vec::new();
    for protocol in 2..=4 {
        let project = cwd.join(format!("retained-v{protocol}"));
        fs::create_dir(&project)?;
        let main = if protocol == 3 {
            "// 😀 preserved\r\nimport { value } from './dep.zry';\r\nexport function main(): i32 { return value(); }\r\n"
        } else if protocol == 4 {
            "struct Pair { left: i32; right: i32; }\r\nexport function main(): i32 { const pair: Pair = Pair { left: 4, right: 3 }; return pair.left + pair.right; }\r\n"
        } else {
            "// 😀 preserved\r\nexport function main(): i32 { return 7; }\r\n"
        };
        fs::write(project.join("main.zry"), main)?;
        if protocol == 3 {
            fs::write(project.join("dep.zry"), "export function value(): i32 { return 7; }\r\n")?;
        }
        let root = WorkspaceSourceRoot::capture(&project)?;
        let source = capture_native_workspace_sources(&root, path("main.zry"))?;
        let map = source.sources().clone();
        let id = map.file_id(&path("main.zry")).ok_or("missing entry")?;
        assert_eq!(map.source(id).expect("entry source").text(), main);
        let marker = cwd.join(format!("retained-v{protocol}.marker"));
        match protocol {
            2 => {
                let syntax = handshake::v2(&map, cwd, "none", &marker)?;
                let native = source.verify_v2()?;
                assert!(native.syntax().is_bound_to(&map));
                let left = zryna_driver::lower_verified_syntax(&syntax, &map)
                    .map_err(|_| "worker M1 lowering rejected")?;
                let right = zryna_driver::lower_verified_syntax(native.syntax(), native.sources())
                    .map_err(|_| "retained M1 lowering rejected")?;
                native.revalidate()?;
                assert_eq!(js::emit(left.program())?, js::emit(right.program())?);
                assert_eq!(
                    wasm::emit(left.program())?.bytes(),
                    wasm::emit(right.program())?.bytes()
                );
                native.revalidate()?;
            }
            3 => {
                let graph = *source.graph_sha256_v3();
                let syntax = handshake::v3(&map, cwd, "none", &marker)?;
                let native = source.verify_v3()?;
                assert_eq!(native.closure().graph_sha256(), &graph);
                assert!(native.closure().syntax().is_bound_to(&map));
                assert_eq!(native.closure().modules().len(), 2);
                let left = m2::lower(
                    m2::SemanticInput::try_new(&syntax, &map, id)
                        .ok_or("worker M2 authority rejected")?,
                )
                .map_err(|_| "worker M2 lowering rejected")?;
                let right = native
                    .closure()
                    .lower_control_flow_v1()
                    .map_err(|_| "retained M2 lowering rejected")?;
                native.revalidate()?;
                assert_eq!(js::emit_control_flow(&left)?, js::emit_control_flow(&right)?);
                assert_eq!(
                    wasm::emit_control_flow(&left)?.bytes(),
                    wasm::emit_control_flow(&right)?.bytes()
                );
                native.revalidate()?;
            }
            _ => {
                let graph = *source.graph_sha256_v4();
                let syntax = handshake::v4(&map, cwd, "none", &marker)?;
                let native = source.verify_v4()?;
                assert_eq!(native.closure().graph_sha256(), &graph);
                assert!(native.closure().syntax().is_bound_to(&map));
                let left = m3::lower(
                    m3::SemanticInput::try_new(&syntax, &map, id)
                        .ok_or("worker M3 authority rejected")?,
                )
                .map_err(|_| "worker M3 lowering rejected")?;
                let right = native
                    .closure()
                    .lower_data_ownership_v1()
                    .map_err(|_| "retained M3 lowering rejected")?;
                native.revalidate()?;
                assert_eq!(
                    js::emit_data_ownership(left.verified_ir(), left.runtime_abi())?,
                    js::emit_data_ownership(right.verified_ir(), right.runtime_abi())?
                );
                assert_eq!(
                    wasm::emit_data_ownership(left.verified_ir(), left.runtime_abi())?.bytes(),
                    wasm::emit_data_ownership(right.verified_ir(), right.runtime_abi())?.bytes()
                );
                native.revalidate()?;
            }
        }
        assert!(marker.is_file());
        passed.push(format!("retained-v{protocol}:dispatch"));
    }
    // Substitution may be denied at write-time on Windows, or rejected at revalidation on Unix.
    let project = cwd.join("retained-stale");
    fs::create_dir(&project)?;
    fs::write(project.join("main.zry"), "export function main(): i32 { return 7; }")?;
    let root = WorkspaceSourceRoot::capture(&project)?;
    let native = capture_native_workspace_sources(&root, path("main.zry"))?.verify_v2()?;
    if fs::write(project.join("main.zry"), "export function main(): i32 { return 8; }").is_ok() {
        assert!(native.revalidate().is_err(), "changed source retained dispatch authority");
    }
    passed.push("retained:stale-source-denied".into());
    Ok(passed)
}
