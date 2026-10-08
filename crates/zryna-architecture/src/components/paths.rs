use crate::contract::WorkspaceContract;
use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use crate::filesystem::exact_relative_entry;
use crate::filesystem::sorted_directory_entries;
use crate::filesystem::valid_component_entry;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub(crate) fn validate_paths(
    root: &Path,
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) {
    for member_root in contract
        .members
        .iter()
        .map(|member| &member.root)
        .chain(contract.adapters.iter().map(|adapter| &adapter.root))
    {
        if diagnostics.is_halted() {
            return;
        }
        if exact_relative_entry(root, Path::new(member_root)).is_none() {
            diagnostics.push(architecture_error(
                "ZRYNA-A1003",
                Some(Path::new(member_root)),
                "component root spelling differs from its registered portable identity",
                "restore every path segment with its exact registered spelling",
            ));
            continue;
        }
        let joined = root.join(member_root);
        match fs::canonicalize(&joined) {
            Ok(canonical) if canonical.starts_with(root) => {}
            Ok(_) => diagnostics.push(architecture_error(
                "ZRYNA-A1202",
                Some(Path::new(member_root)),
                "component root escapes the canonical workspace",
                "place the component inside the workspace without symlink indirection",
            )),
            Err(error) => diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new(member_root)),
                format!("registered component root is unavailable: {error}"),
                "create it with the canonical project planner",
            )),
        }
    }
}

pub(crate) fn validate_component_entries(
    root: &Path,
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) {
    for (component_root, allowed_entries, allows_node_modules) in
        contract.members.iter().map(|member| (&member.root, &member.allowed_entries, false)).chain(
            contract.adapters.iter().map(|adapter| (&adapter.root, &adapter.allowed_entries, true)),
        )
    {
        if diagnostics.is_halted() {
            return;
        }
        let mut allowed = BTreeSet::new();
        let mut portable_identities = BTreeSet::new();
        for entry in allowed_entries {
            if diagnostics.is_halted() {
                return;
            }
            if !valid_component_entry(entry)
                || !allowed.insert(entry.as_str())
                || !portable_identities.insert(entry.to_ascii_lowercase())
            {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1003",
                    Some(Path::new(component_root)),
                    format!("allowed component entry '{entry}' is invalid or duplicated"),
                    "use each portable immediate file or directory name exactly once",
                ));
            }
        }
        let path = root.join(component_root);
        let Some(entries) = sorted_directory_entries(&path, Path::new(component_root), diagnostics)
        else {
            continue;
        };
        for entry in entries {
            if diagnostics.is_halted() {
                return;
            }
            let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1203",
                    entry.path().strip_prefix(root).ok(),
                    "component entry name is not valid UTF-8",
                    "rename the entry with a portable UTF-8 name",
                ));
                continue;
            };
            if allows_node_modules && name == "node_modules" {
                match entry.file_type() {
                    Ok(file_type) if file_type.is_dir() => continue,
                    Ok(_) => {}
                    Err(error) => {
                        diagnostics.push(architecture_error(
                            "ZRYNA-A1205",
                            entry.path().strip_prefix(root).ok(),
                            format!("component entry type could not be inspected: {error}"),
                            "restore directory consistency and retry",
                        ));
                        continue;
                    }
                }
            }
            if !allowed.contains(name.as_str()) {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1007",
                    entry.path().strip_prefix(root).ok(),
                    format!("'{name}' is outside the registered component layout"),
                    "move it into an allowed entry or update zryna.workspace.json deliberately",
                ));
            }
        }
    }
}
