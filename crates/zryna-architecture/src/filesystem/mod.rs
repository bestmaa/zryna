pub(super) mod entries;
pub(super) mod identity;
pub(super) mod read;
pub(super) mod scan;

use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use crate::filesystem::identity::metadata_is_link_or_reparse;
use crate::filesystem::scan::MAX_SCAN_ENTRIES;
use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use zryna_diagnostics::Diagnostic;

pub(crate) fn canonical_workspace_root(root: &Path) -> Result<PathBuf, Diagnostic> {
    let metadata = fs::symlink_metadata(root).map_err(|error| {
        architecture_error(
            "ZRYNA-A1001",
            Some(root),
            format!("workspace root is unavailable: {error}"),
            "select an existing regular directory containing zryna.workspace.json",
        )
    })?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(architecture_error(
            "ZRYNA-A1201",
            Some(root),
            "workspace root must be a real, non-symlink directory",
            "open the canonical project directory directly",
        ));
    }
    fs::canonicalize(root).map_err(|error| {
        architecture_error(
            "ZRYNA-A1001",
            Some(root),
            format!("workspace root cannot be canonicalized: {error}"),
            "fix workspace permissions and path components",
        )
    })
}

pub(crate) fn sorted_directory_entries(
    path: &Path,
    diagnostic_path: &Path,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<Vec<fs::DirEntry>> {
    let entries = match fs::read_dir(path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1205",
                Some(diagnostic_path),
                format!("directory scan failed: {error}"),
                "restore read access; incomplete scans never pass",
            ));
            return None;
        }
    };
    let mut sorted = Vec::new();
    for entry in entries {
        if diagnostics.is_halted() {
            return None;
        }
        if sorted.len() >= MAX_SCAN_ENTRIES {
            diagnostics.halt(architecture_error(
                "ZRYNA-A1204",
                Some(diagnostic_path),
                "directory validation exceeded its deterministic entry budget",
                "reduce controlled entries; incomplete validation never passes",
            ));
            return None;
        }
        match entry {
            Ok(value) => sorted.push(value),
            Err(error) => diagnostics.push(architecture_error(
                "ZRYNA-A1205",
                Some(diagnostic_path),
                format!("directory entry could not be inspected: {error}"),
                "restore directory consistency and retry",
            )),
        }
    }
    sorted.sort_by_key(fs::DirEntry::file_name);
    Some(sorted)
}

pub(crate) fn exact_relative_entry(root: &Path, relative: &Path) -> Option<PathBuf> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(expected) = component else {
            return None;
        };
        let entries = fs::read_dir(&current).ok()?;
        let mut matched = None;
        for entry in entries {
            let entry = entry.ok()?;
            if entry.file_name() == expected {
                matched = Some(entry.path());
                break;
            }
        }
        current = matched?;
    }
    Some(current)
}

pub(crate) fn exact_regular_file(root: &Path, relative: &Path) -> bool {
    exact_relative_entry(root, relative).is_some_and(|path| {
        fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
    })
}

pub(crate) fn safe_relative_path(value: &str) -> bool {
    if value.is_empty()
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains("//")
        || value.contains('\\')
    {
        return false;
    }
    value.split('/').all(portable_path_segment)
}

pub(crate) fn valid_component_entry(value: &str) -> bool {
    !value.contains(['/', '\\']) && portable_path_segment(value)
}

pub(crate) fn portable_path_segment(value: &str) -> bool {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.len() > 255
        || value.ends_with(['.', ' '])
        || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
        || value.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*'])
    {
        return false;
    }
    let stem = value.split('.').next().unwrap_or(value).to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

pub(crate) fn valid_id(value: &str) -> bool {
    value.bytes().next().is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || (byte == b'-' && index > 0)
        })
        && !value.ends_with('-')
        && !value.contains("--")
}
