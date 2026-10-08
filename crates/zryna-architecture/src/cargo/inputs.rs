use crate::components::manifests::MAX_MANIFEST_BYTES;
use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use crate::filesystem::exact_relative_entry;
use crate::filesystem::read::read_bounded_utf8;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub(crate) struct CargoInputSnapshot {
    pub(crate) relative_path: PathBuf,
    pub(crate) source: Option<String>,
    pub(crate) max_bytes: u64,
}

pub(crate) fn read_required_cargo_input_snapshot(
    root: &Path,
    relative: &str,
    max_bytes: u64,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<CargoInputSnapshot> {
    let relative_path = PathBuf::from(relative);
    let path = root.join(&relative_path);
    match read_bounded_utf8(
        &path,
        Some(&relative_path),
        max_bytes,
        "ZRYNA-A1005",
        "restore the required Cargo graph input",
    ) {
        Ok((source, _)) => {
            Some(CargoInputSnapshot { relative_path, source: Some(source), max_bytes })
        }
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            None
        }
    }
}

pub(crate) fn read_optional_cargo_input_snapshots(
    root: &Path,
    snapshots: &mut Vec<CargoInputSnapshot>,
    diagnostics: &mut ValidationDiagnostics,
) {
    for relative in [".cargo/config.toml", ".cargo/config"] {
        if diagnostics.is_halted() {
            return;
        }
        let relative_path = PathBuf::from(relative);
        let path = root.join(&relative_path);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(noncanonical) = find_noncanonical_case_entry(root, &relative_path) {
                    diagnostics.push(architecture_error(
                        "ZRYNA-A1003",
                        Some(&noncanonical),
                        "Cargo configuration input has noncanonical filesystem spelling",
                        "use the exact portable .cargo/config.toml or .cargo/config spelling",
                    ));
                    continue;
                }
                snapshots.push(CargoInputSnapshot {
                    relative_path,
                    source: None,
                    max_bytes: MAX_MANIFEST_BYTES,
                });
                continue;
            }
            Err(error) => {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1205",
                    Some(&relative_path),
                    format!("Cargo configuration input could not be inspected: {error}"),
                    "restore stable repository-local Cargo configuration",
                ));
                continue;
            }
        };
        if exact_relative_entry(root, &relative_path).is_none() {
            diagnostics.push(architecture_error(
                "ZRYNA-A1003",
                Some(&relative_path),
                "Cargo configuration input has noncanonical filesystem spelling",
                "use the exact portable .cargo/config.toml or .cargo/config spelling",
            ));
            continue;
        }
        if !metadata.file_type().is_file() {
            diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(&relative_path),
                "Cargo configuration input is not a regular file",
                "restore the canonical repository-local Cargo configuration file",
            ));
            continue;
        }
        match read_bounded_utf8(
            &path,
            Some(&relative_path),
            MAX_MANIFEST_BYTES,
            "ZRYNA-A1203",
            "restore stable repository-local Cargo configuration",
        ) {
            Ok((source, _)) => snapshots.push(CargoInputSnapshot {
                relative_path,
                source: Some(source),
                max_bytes: MAX_MANIFEST_BYTES,
            }),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }
}

fn find_noncanonical_case_entry(root: &Path, relative: &Path) -> Option<PathBuf> {
    let parent = relative.parent()?;
    let expected = relative.file_name()?.to_str()?;
    let parent_path = exact_relative_entry(root, parent)?;
    let mut names: Vec<OsString> = fs::read_dir(parent_path)
        .ok()?
        .filter_map(|entry| entry.ok().map(|value| value.file_name()))
        .collect();
    names.sort();
    names.into_iter().find_map(|name| {
        let value = name.to_str()?;
        (value != expected && value.eq_ignore_ascii_case(expected)).then(|| parent.join(value))
    })
}

pub(crate) fn validate_cargo_inputs_unchanged(
    root: &Path,
    snapshots: &[CargoInputSnapshot],
    diagnostics: &mut ValidationDiagnostics,
) {
    for snapshot in snapshots {
        if diagnostics.is_halted() {
            return;
        }
        let path = root.join(&snapshot.relative_path);
        if let Some(expected_source) = &snapshot.source {
            match read_bounded_utf8(
                &path,
                Some(&snapshot.relative_path),
                snapshot.max_bytes,
                "ZRYNA-A1203",
                "stop concurrent Cargo input mutation and retry architecture validation",
            ) {
                Ok((source, _)) if source == *expected_source => {}
                Ok(_) => diagnostics.push(architecture_error(
                    "ZRYNA-A1203",
                    Some(&snapshot.relative_path),
                    "Cargo graph input changed during architecture validation",
                    "stop concurrent Cargo input mutation and retry architecture validation",
                )),
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
            continue;
        }
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if find_noncanonical_case_entry(root, &snapshot.relative_path).is_some() {
                    diagnostics.push(architecture_error(
                        "ZRYNA-A1203",
                        Some(&snapshot.relative_path),
                        "noncanonical Cargo graph input was created during validation",
                        "stop concurrent Cargo input mutation and retry architecture validation",
                    ));
                }
            }
            Ok(_) => diagnostics.push(architecture_error(
                "ZRYNA-A1203",
                Some(&snapshot.relative_path),
                "Cargo graph input was created during architecture validation",
                "stop concurrent Cargo input mutation and retry architecture validation",
            )),
            Err(error) => diagnostics.push(architecture_error(
                "ZRYNA-A1205",
                Some(&snapshot.relative_path),
                format!("Cargo graph input state could not be inspected: {error}"),
                "restore stable Cargo input paths and retry architecture validation",
            )),
        }
    }
}
