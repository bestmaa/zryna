use crate::cargo::inputs::CargoInputSnapshot;
use crate::contract::CONTRACT_FILE;
use crate::contract::MemberKind;
use crate::contract::WorkspaceContract;
use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use crate::filesystem::exact_regular_file;
use crate::filesystem::read::read_bounded_utf8;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

pub(crate) const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub(crate) fn validate_members(
    root: &Path,
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) -> Vec<CargoInputSnapshot> {
    let mut snapshots = Vec::new();
    let root_manifest_path = root.join("Cargo.toml");
    let root_manifest = read_toml_with_source(&root_manifest_path, diagnostics);
    if let Some((manifest, source)) = root_manifest {
        snapshots.push(CargoInputSnapshot {
            relative_path: PathBuf::from("Cargo.toml"),
            source: Some(source),
            max_bytes: MAX_MANIFEST_BYTES,
        });
        let cargo_members: BTreeSet<String> = manifest
            .get("workspace")
            .and_then(|workspace| workspace.get("members"))
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(toml::Value::as_str)
            .map(ToOwned::to_owned)
            .collect();
        let contract_members: BTreeSet<String> =
            contract.members.iter().map(|member| member.root.clone()).collect();
        if cargo_members != contract_members {
            diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new("Cargo.toml")),
                "Cargo workspace members differ from zryna.workspace.json",
                "register every Rust member exactly once in both authoritative manifests",
            ));
        }
    }
    if diagnostics.is_halted() {
        return snapshots;
    }

    for member in &contract.members {
        if diagnostics.is_halted() {
            return snapshots;
        }
        let member_root = root.join(&member.root);
        let manifest_path = member_root.join("Cargo.toml");
        let manifest_relative = Path::new(&member.root).join("Cargo.toml");
        let readme_relative = Path::new(&member.root).join("README.md");
        if !exact_regular_file(root, &readme_relative) {
            diagnostics.push(architecture_error(
                "ZRYNA-A1006",
                Some(Path::new(&member.root)),
                "registered member is missing README.md",
                "document the component authority and dependency boundary",
            ));
        }
        let expected_entry = if member.kind == MemberKind::Application {
            Path::new(&member.root).join("src/main.rs")
        } else {
            Path::new(&member.root).join("src/lib.rs")
        };
        if !exact_regular_file(root, &expected_entry) {
            diagnostics.push(architecture_error(
                "ZRYNA-A1006",
                Some(Path::new(&member.root)),
                "registered member has the wrong Rust entrypoint",
                "applications require src/main.rs; all other members require src/lib.rs",
            ));
        }
        if diagnostics.is_halted() {
            return snapshots;
        }
        if !exact_regular_file(root, &manifest_relative) {
            diagnostics.push(architecture_error(
                "ZRYNA-A1006",
                Some(&manifest_relative),
                "registered member Cargo.toml is missing or has noncanonical spelling",
                "restore the exact regular Cargo.toml file",
            ));
            continue;
        }
        let Some((manifest, source)) = read_toml_with_source(&manifest_path, diagnostics) else {
            continue;
        };
        snapshots.push(CargoInputSnapshot {
            relative_path: PathBuf::from(&member.root).join("Cargo.toml"),
            source: Some(source),
            max_bytes: MAX_MANIFEST_BYTES,
        });
        let package_name = manifest
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str);
        if package_name != Some(member.id.as_str()) {
            diagnostics.push(architecture_error(
                "ZRYNA-A1006",
                Some(&manifest_path),
                format!("Cargo package name does not match registered id '{}'", member.id),
                "make the directory, member id, and Cargo package name identical",
            ));
        }
    }
    snapshots
}

pub(crate) fn validate_adapters(
    root: &Path,
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) {
    for adapter in &contract.adapters {
        if diagnostics.is_halted() {
            return;
        }
        let adapter_root = root.join(&adapter.root);
        let readme_path = Path::new(&adapter.root).join("README.md");
        let worker_path = Path::new(&adapter.root).join("src/worker.mjs");
        let package_path = adapter_root.join("package.json");
        let package_relative = Path::new(&adapter.root).join("package.json");
        if !exact_regular_file(root, &readme_path)
            || !exact_regular_file(root, &worker_path)
            || !exact_regular_file(root, &package_relative)
        {
            diagnostics.push(architecture_error(
                "ZRYNA-A1010",
                Some(Path::new(&adapter.root)),
                "registered frontend adapter is missing README.md or src/worker.mjs",
                "restore the documented newline-JSON worker boundary",
            ));
        }
        if diagnostics.is_halted() {
            return;
        }
        let Some(package) = read_json(&package_path, diagnostics) else {
            continue;
        };
        let expected_name = format!("@zryna/adapter-{}", adapter.id);
        if package.get("name").and_then(serde_json::Value::as_str) != Some(expected_name.as_str()) {
            diagnostics.push(architecture_error(
                "ZRYNA-A1010",
                Some(&package_path),
                format!("adapter package name must be '{expected_name}'"),
                "make the package identity derive from the registered adapter id",
            ));
        }
        let metadata = package.get("zryna");
        let metadata_id =
            metadata.and_then(|value| value.get("adapterId")).and_then(serde_json::Value::as_str);
        let metadata_protocol = metadata
            .and_then(|value| value.get("protocolVersion"))
            .and_then(serde_json::Value::as_u64);
        let metadata_worker =
            metadata.and_then(|value| value.get("worker")).and_then(serde_json::Value::as_str);
        if metadata_id != Some(adapter.id.as_str())
            || metadata_protocol != Some(u64::from(adapter.protocol_version))
            || metadata_worker != Some("src/worker.mjs")
        {
            diagnostics.push(architecture_error(
                "ZRYNA-A1010",
                Some(&package_path),
                "adapter metadata differs from zryna.workspace.json",
                "set zryna.adapterId, zryna.protocolVersion, and zryna.worker to the registered values",
            ));
        }
        let Some((toolchain_name, toolchain_version)) = adapter.toolchain.rsplit_once('@') else {
            diagnostics.push(architecture_error(
                "ZRYNA-A1011",
                Some(Path::new(CONTRACT_FILE)),
                format!(
                    "adapter toolchain '{}' is not an exact package@version",
                    adapter.toolchain
                ),
                "pin one exact frontend package version in the architecture contract",
            ));
            continue;
        };
        let dependency_version = package
            .get("dependencies")
            .and_then(|value| value.get(toolchain_name))
            .and_then(serde_json::Value::as_str);
        if toolchain_name.is_empty()
            || toolchain_version.is_empty()
            || dependency_version != Some(toolchain_version)
        {
            diagnostics.push(architecture_error(
                "ZRYNA-A1011",
                Some(&package_path),
                format!("adapter must pin toolchain '{}' exactly", adapter.toolchain),
                "make package.json dependencies match the registered package and version",
            ));
        }
    }
}

pub(crate) fn read_toml_with_source(
    path: &Path,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<(toml::Value, String)> {
    let (source, _) = match read_bounded_utf8(
        path,
        Some(path),
        MAX_MANIFEST_BYTES,
        "ZRYNA-A1005",
        "restore the canonical component Cargo.toml",
    ) {
        Ok(value) => value,
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            return None;
        }
    };
    match toml::from_str(&source) {
        Ok(value) => Some((value, source)),
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1006",
                Some(path),
                format!("Cargo manifest is invalid: {error}"),
                "repair the manifest before architecture validation",
            ));
            None
        }
    }
}

fn read_json(path: &Path, diagnostics: &mut ValidationDiagnostics) -> Option<serde_json::Value> {
    let (source, _) = match read_bounded_utf8(
        path,
        Some(path),
        MAX_MANIFEST_BYTES,
        "ZRYNA-A1010",
        "restore the registered adapter package.json",
    ) {
        Ok(value) => value,
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            return None;
        }
    };
    match serde_json::from_str(&source) {
        Ok(value) => Some(value),
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1010",
                Some(path),
                format!("adapter package manifest is invalid JSON: {error}"),
                "repair package.json before architecture validation",
            ));
            None
        }
    }
}
