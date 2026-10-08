use crate::contract::WorkspaceContract;
use crate::diagnostics::MAX_SCAN_DIAGNOSTICS;
use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use crate::filesystem::entries::scan_directory;
use crate::filesystem::entries::scan_regular_file;
use crate::filesystem::entries::validate_excluded_entry;
use crate::filesystem::identity::metadata_is_link_or_reparse;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use zryna_diagnostics::Diagnostic;

const MAX_SCAN_DEPTH: usize = 32;
pub(crate) const MAX_SCAN_ENTRIES: usize = 50_000;
const MAX_SCAN_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_SCANNED_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(crate) struct ScanLimits {
    pub(crate) entries: usize,
    pub(crate) depth: usize,
    pub(crate) file_bytes: u64,
    pub(crate) total_bytes: u64,
    pub(crate) diagnostics: usize,
}

const PRODUCTION_SCAN_LIMITS: ScanLimits = ScanLimits {
    entries: MAX_SCAN_ENTRIES,
    depth: MAX_SCAN_DEPTH,
    file_bytes: MAX_SCANNED_FILE_BYTES,
    total_bytes: MAX_SCAN_TOTAL_BYTES,
    diagnostics: MAX_SCAN_DIAGNOSTICS,
};

pub(crate) struct ScanState {
    pub(crate) limits: ScanLimits,
    pub(crate) entries_seen: usize,
    pub(crate) bytes_seen: u64,
    pub(crate) diagnostics_seen: usize,
    pub(crate) halted: bool,
}

pub(crate) struct ScanPolicy<'a> {
    pub(crate) exclusions: BTreeMap<PathBuf, bool>,
    pub(crate) contract_source: &'a str,
}

impl<'a> ScanPolicy<'a> {
    pub(crate) fn new(contract: &WorkspaceContract, contract_source: &'a str) -> Self {
        let mut exclusions =
            BTreeMap::from([(PathBuf::from(".git"), false), (PathBuf::from("node_modules"), true)]);
        for output in &contract.outputs {
            exclusions.insert(PathBuf::from(output), true);
        }
        for adapter in &contract.adapters {
            exclusions.insert(Path::new(&adapter.root).join("node_modules"), true);
        }
        Self { exclusions, contract_source }
    }

    pub(crate) fn excluded_shape(&self, relative: &Path) -> Option<bool> {
        self.exclusions.get(relative).copied()
    }
}

impl ScanState {
    pub(crate) const fn new(limits: ScanLimits) -> Self {
        Self { limits, entries_seen: 0, bytes_seen: 0, diagnostics_seen: 0, halted: false }
    }
}

pub(crate) fn validate_bounded_filesystem(
    root: &Path,
    contract: &WorkspaceContract,
    contract_source: &str,
    diagnostics: &mut ValidationDiagnostics,
) -> bool {
    let policy = ScanPolicy::new(contract, contract_source);
    let mut state = ScanState::new(PRODUCTION_SCAN_LIMITS);
    scan_path(root, root, &policy, 0, &mut state, diagnostics);
    !state.halted
}

pub(crate) fn scan_path(
    root: &Path,
    path: &Path,
    policy: &ScanPolicy<'_>,
    depth: usize,
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
) {
    if state.halted {
        return;
    }
    if depth > state.limits.depth {
        halt_scan(
            state,
            diagnostics,
            path.strip_prefix(root).ok(),
            "architecture scan exceeded its deterministic depth budget",
        );
        return;
    }
    if state.entries_seen >= state.limits.entries {
        halt_scan(
            state,
            diagnostics,
            path.strip_prefix(root).ok(),
            "architecture scan exceeded its deterministic entry budget",
        );
        return;
    }
    state.entries_seen += 1;

    let relative = path.strip_prefix(root).unwrap_or(path);
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) => {
            push_scan_diagnostic(
                state,
                diagnostics,
                architecture_error(
                    "ZRYNA-A1205",
                    Some(relative),
                    format!("filesystem entry could not be inspected: {error}"),
                    "restore a stable readable entry and retry",
                ),
            );
            return;
        }
    };
    if metadata_is_link_or_reparse(&metadata) {
        push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1201",
                Some(relative),
                "symlinks are forbidden inside the controlled workspace",
                "replace the link with a real in-workspace file or directory",
            ),
        );
        return;
    }

    if let Some(expected_directory) = policy.excluded_shape(relative) {
        validate_excluded_entry(relative, &metadata, expected_directory, state, diagnostics);
        return;
    }

    if metadata.is_file() {
        scan_regular_file(path, relative, &metadata, policy, state, diagnostics);
        return;
    }
    if !metadata.is_dir() {
        push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1201",
                Some(relative),
                "non-regular filesystem entries are forbidden",
                "remove sockets, devices, and FIFOs from controlled source roots",
            ),
        );
        return;
    }

    scan_directory(root, path, relative, policy, depth, state, diagnostics);
}

pub(crate) fn push_scan_diagnostic(
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
    diagnostic: Diagnostic,
) {
    if state.halted {
        return;
    }
    if state.diagnostics_seen.saturating_add(1) >= state.limits.diagnostics {
        halt_scan(
            state,
            diagnostics,
            None,
            "architecture scan exceeded its deterministic diagnostic budget",
        );
        return;
    }
    diagnostics.push(diagnostic);
    state.diagnostics_seen += 1;
    if diagnostics.is_halted() {
        state.halted = true;
    }
}

pub(crate) fn halt_scan(
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
    path: Option<&Path>,
    message: &str,
) {
    if state.halted {
        return;
    }
    halt_scan_with_diagnostic(
        state,
        diagnostics,
        architecture_error(
            "ZRYNA-A1204",
            path,
            message,
            "reduce controlled input size or depth; incomplete scans never pass",
        ),
    );
}

pub(crate) fn halt_scan_with_diagnostic(
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
    diagnostic: Diagnostic,
) {
    if state.halted {
        return;
    }
    if state.diagnostics_seen < state.limits.diagnostics {
        diagnostics.push(diagnostic);
        state.diagnostics_seen += 1;
    }
    state.halted = true;
}
