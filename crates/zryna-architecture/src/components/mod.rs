pub(super) mod manifests;
pub(super) mod paths;

use crate::contract::CONTRACT_FILE;
use crate::contract::MemberKind;
use crate::contract::WorkspaceContract;
use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use crate::filesystem::sorted_directory_entries;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const ALLOWED_ROOT_ENTRIES: &[&str] = &[
    ".cargo",
    ".git",
    ".github",
    ".gitattributes",
    ".gitignore",
    ".zryna",
    "CODE_OF_CONDUCT.md",
    "CONTRIBUTING.md",
    "Cargo.lock",
    "Cargo.toml",
    "LICENSE",
    "NOTICE",
    "README.md",
    "SECURITY.md",
    "adapters",
    "apps",
    "crates",
    "docs",
    "editors",
    "examples",
    "node_modules",
    "package.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "runtime",
    "rust-toolchain.toml",
    "rustfmt.toml",
    "schemas",
    "scripts",
    "spec",
    "target",
    "tests",
    "toolchains",
    "zryna.workspace.json",
];

const REQUIRED_ROOT_DIRECTORIES: &[&str] = &["adapters", "apps", "crates"];

const REQUIRED_ROOT_FILES: &[&str] =
    &["Cargo.lock", "Cargo.toml", "rust-toolchain.toml", CONTRACT_FILE];
pub(crate) fn validate_root_entries(root: &Path, diagnostics: &mut ValidationDiagnostics) {
    let allowed: BTreeSet<&str> = ALLOWED_ROOT_ENTRIES.iter().copied().collect();
    let portable_allowed: BTreeMap<String, &str> =
        ALLOWED_ROOT_ENTRIES.iter().map(|value| (value.to_ascii_lowercase(), *value)).collect();
    let Some(entries) = sorted_directory_entries(root, root, diagnostics) else {
        return;
    };
    for entry in entries {
        if diagnostics.is_halted() {
            return;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            diagnostics.push(architecture_error(
                "ZRYNA-A1203",
                Some(&entry.path()),
                "workspace entry name is not valid UTF-8",
                "rename the entry with a portable UTF-8 name",
            ));
            continue;
        };
        if !allowed.contains(name) {
            if let Some(expected) = portable_allowed.get(&name.to_ascii_lowercase()) {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1003",
                    Some(Path::new(name)),
                    format!("root entry '{name}' has noncanonical spelling; expected '{expected}'"),
                    "rename the entry to its exact portable spelling",
                ));
                continue;
            }
            diagnostics.push(architecture_error(
                "ZRYNA-A1004",
                Some(Path::new(name)),
                format!("root entry '{name}' is not part of the Zryna architecture"),
                "move the content into a registered component or redefine the contract deliberately",
            ));
        }
    }
}

pub(crate) fn validate_required_root_shapes(root: &Path, diagnostics: &mut ValidationDiagnostics) {
    for relative in REQUIRED_ROOT_FILES {
        if diagnostics.is_halted() {
            return;
        }
        let path = root.join(relative);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            Ok(_) => diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new(relative)),
                format!("required root entry '{relative}' is not a regular file"),
                "restore the canonical root file shape",
            )),
            Err(error) => diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new(relative)),
                format!("required root file '{relative}' is unavailable: {error}"),
                "restore the canonical root file",
            )),
        }
    }
    for relative in REQUIRED_ROOT_DIRECTORIES {
        if diagnostics.is_halted() {
            return;
        }
        let path = root.join(relative);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new(relative)),
                format!("required root entry '{relative}' is not a directory"),
                "restore the canonical root directory shape",
            )),
            Err(error) => diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new(relative)),
                format!("required root directory '{relative}' is unavailable: {error}"),
                "restore the canonical root directory",
            )),
        }
    }
}

pub(crate) fn validate_component_containers(
    root: &Path,
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) {
    let applications: BTreeSet<String> = contract
        .members
        .iter()
        .filter(|member| member.kind == MemberKind::Application)
        .map(|member| member.id.clone())
        .collect();
    let libraries: BTreeSet<String> = contract
        .members
        .iter()
        .filter(|member| member.kind != MemberKind::Application)
        .map(|member| member.id.clone())
        .collect();
    let adapters: BTreeSet<String> =
        contract.adapters.iter().map(|adapter| adapter.id.clone()).collect();
    for (container, expected) in
        [("apps", applications), ("crates", libraries), ("adapters", adapters)]
    {
        if diagnostics.is_halted() {
            return;
        }
        let container_path = root.join(container);
        let Some(entries) =
            sorted_directory_entries(&container_path, Path::new(container), diagnostics)
        else {
            continue;
        };
        let portable_expected: BTreeMap<String, &str> =
            expected.iter().map(|value| (value.to_ascii_lowercase(), value.as_str())).collect();
        let mut actual = BTreeSet::new();
        for entry in entries {
            if diagnostics.is_halted() {
                return;
            }
            let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1203",
                    entry.path().strip_prefix(root).ok(),
                    "component directory name is not valid UTF-8",
                    "rename the directory with its registered printable ASCII id",
                ));
                continue;
            };
            let relative = Path::new(container).join(&name);
            if expected.contains(name.as_str()) {
                match entry.file_type() {
                    Ok(file_type) if file_type.is_dir() => {
                        actual.insert(name);
                    }
                    Ok(_) => diagnostics.push(architecture_error(
                        "ZRYNA-A1005",
                        Some(&relative),
                        "registered component root is not a directory",
                        "restore the registered component directory",
                    )),
                    Err(error) => diagnostics.push(architecture_error(
                        "ZRYNA-A1205",
                        Some(&relative),
                        format!("component root type could not be inspected: {error}"),
                        "restore directory consistency and retry",
                    )),
                }
                continue;
            }
            if let Some(canonical) = portable_expected.get(&name.to_ascii_lowercase()) {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1003",
                    Some(&relative),
                    format!(
                        "component root '{container}/{name}' has noncanonical spelling; expected '{container}/{canonical}'"
                    ),
                    "rename the directory to its exact registered portable spelling",
                ));
            } else {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1005",
                    Some(&relative),
                    format!("unregistered component root '{container}/{name}'"),
                    "register the component deliberately or remove it from the controlled container",
                ));
            }
        }
        for missing in expected.difference(&actual) {
            if diagnostics.is_halted() {
                return;
            }
            diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(&Path::new(container).join(missing)),
                format!("registered component root '{container}/{missing}' is missing"),
                "restore the exact registered component directory",
            ));
        }
    }
}
