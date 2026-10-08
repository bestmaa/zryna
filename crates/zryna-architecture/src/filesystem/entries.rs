use crate::contract::CONTRACT_FILE;
use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use crate::filesystem::portable_path_segment;
use crate::filesystem::read::read_bounded_utf8_with_expected_size;
use crate::filesystem::scan::ScanPolicy;
use crate::filesystem::scan::ScanState;
use crate::filesystem::scan::halt_scan;
use crate::filesystem::scan::halt_scan_with_diagnostic;
use crate::filesystem::scan::push_scan_diagnostic;
use crate::filesystem::scan::scan_path;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(crate) fn validate_excluded_entry(
    relative: &Path,
    metadata: &fs::Metadata,
    expected_directory: bool,
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
) {
    if expected_directory && !metadata.is_dir() {
        push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1201",
                Some(relative),
                "declared generated output must be a real directory",
                "replace it with the declared non-symlink output directory",
            ),
        );
    } else if !expected_directory && !metadata.is_dir() && !metadata.is_file() {
        push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1201",
                Some(relative),
                "Git metadata must be a real file or directory",
                "replace it with regular Git metadata",
            ),
        );
    }
}

pub(crate) fn scan_regular_file(
    path: &Path,
    relative: &Path,
    metadata: &fs::Metadata,
    policy: &ScanPolicy<'_>,
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
) {
    let Some(advertised_total) = state.bytes_seen.checked_add(metadata.len()) else {
        halt_scan(
            state,
            diagnostics,
            Some(relative),
            "architecture scan byte accounting overflowed",
        );
        return;
    };
    if advertised_total > state.limits.total_bytes {
        halt_scan(
            state,
            diagnostics,
            Some(relative),
            "architecture scan exceeded its deterministic aggregate byte budget",
        );
        return;
    }
    state.bytes_seen = advertised_total;
    match read_bounded_utf8_with_expected_size(
        path,
        Some(relative),
        state.limits.file_bytes,
        "ZRYNA-A1203",
        "restore the controlled UTF-8 source file",
        metadata.len(),
    ) {
        Ok((source, _)) => {
            if relative == Path::new(CONTRACT_FILE) && source != policy.contract_source {
                halt_scan_with_diagnostic(
                    state,
                    diagnostics,
                    architecture_error(
                        "ZRYNA-A1203",
                        Some(relative),
                        "workspace contract changed after it was parsed",
                        "stop concurrent mutation and retry architecture validation",
                    ),
                );
            }
        }
        Err(diagnostic) if diagnostic.code == "ZRYNA-A1204" => {
            halt_scan_with_diagnostic(state, diagnostics, diagnostic);
        }
        Err(diagnostic) => push_scan_diagnostic(state, diagnostics, diagnostic),
    }
}

pub(crate) fn scan_directory(
    root: &Path,
    path: &Path,
    relative: &Path,
    policy: &ScanPolicy<'_>,
    depth: usize,
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
) {
    let children = match fs::read_dir(path) {
        Ok(value) => value,
        Err(error) => {
            push_scan_diagnostic(
                state,
                diagnostics,
                architecture_error(
                    "ZRYNA-A1205",
                    Some(relative),
                    format!("directory scan failed: {error}"),
                    "restore read access; incomplete scans never pass",
                ),
            );
            return;
        }
    };
    let remaining_entries = state.limits.entries.saturating_sub(state.entries_seen);
    let mut child_entries = Vec::new();
    let mut read_failed = false;
    for child in children {
        match child {
            Ok(entry) => {
                if child_entries.len() >= remaining_entries {
                    halt_scan(
                        state,
                        diagnostics,
                        Some(relative),
                        "architecture scan exceeded its deterministic entry budget",
                    );
                    return;
                }
                child_entries.push(entry);
            }
            Err(_) => read_failed = true,
        }
    }
    if read_failed {
        push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1205",
                Some(relative),
                "one or more directory entries could not be read",
                "restore directory consistency and retry",
            ),
        );
        return;
    }
    child_entries.sort_by_key(fs::DirEntry::file_name);
    let mut child_paths = Vec::with_capacity(child_entries.len());
    let mut portable_identities = BTreeMap::new();
    for entry in child_entries {
        validate_portable_directory_entry(
            root,
            &entry,
            &mut portable_identities,
            state,
            diagnostics,
        );
        child_paths.push(entry.path());
    }
    child_paths.sort();
    for child_path in child_paths {
        if state.halted {
            break;
        }
        scan_path(root, &child_path, policy, depth + 1, state, diagnostics);
    }
}

fn validate_portable_directory_entry(
    root: &Path,
    entry: &fs::DirEntry,
    portable_identities: &mut BTreeMap<String, String>,
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
) {
    let name = entry.file_name();
    match name.to_str() {
        Some(value) if portable_path_segment(value) => {
            let identity = value.to_ascii_lowercase();
            if let Some(previous) = portable_identities.insert(identity, value.to_owned()) {
                push_scan_diagnostic(
                    state,
                    diagnostics,
                    architecture_error(
                        "ZRYNA-A1003",
                        entry.path().strip_prefix(root).ok(),
                        format!(
                            "filesystem entries '{previous}' and '{value}' collide under the portable path identity"
                        ),
                        "keep one printable ASCII spelling for every controlled entry",
                    ),
                );
            }
        }
        Some(value) => push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1003",
                entry.path().strip_prefix(root).ok(),
                format!("filesystem entry name '{value}' is not portable"),
                "use printable ASCII without reserved names, characters, or trailing dots and spaces",
            ),
        ),
        None => push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1203",
                entry.path().strip_prefix(root).ok(),
                "filesystem entry name is not valid UTF-8",
                "rename the entry with a portable printable ASCII name",
            ),
        ),
    }
}
