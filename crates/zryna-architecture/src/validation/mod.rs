use crate::cargo::inputs::read_optional_cargo_input_snapshots;
use crate::cargo::inputs::read_required_cargo_input_snapshot;
use crate::cargo::inputs::validate_cargo_inputs_unchanged;
use crate::cargo::process::load_cargo_metadata;
use crate::cargo::validate_resolved_cargo_graph;
use crate::components::manifests::MAX_MANIFEST_BYTES;
use crate::components::manifests::validate_adapters;
use crate::components::manifests::validate_members;
use crate::components::paths::validate_component_entries;
use crate::components::paths::validate_paths;
use crate::components::validate_component_containers;
use crate::components::validate_required_root_shapes;
use crate::components::validate_root_entries;
use crate::contract::load_contract;
use crate::contract::validate_contract_identity;
use crate::contract::validate_contract_paths;
use crate::contract::validate_contract_unchanged;
use crate::dependency_graph::validate_dependency_graph;
use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::ValidationReport;
use crate::diagnostics::validation_report;
use crate::filesystem::canonical_workspace_root;
use crate::filesystem::scan::MAX_SCANNED_FILE_BYTES;
use crate::filesystem::scan::validate_bounded_filesystem;
use std::path::Path;

/// Validates a complete Zryna workspace and fails closed on incomplete inspection.
#[must_use]
pub fn validate_workspace(root: &Path) -> ValidationReport {
    let mut diagnostics = ValidationDiagnostics::default();
    let canonical_root = match canonical_workspace_root(root) {
        Ok(value) => value,
        Err(diagnostic) => return ValidationReport { diagnostics: vec![diagnostic] },
    };
    let (contract, contract_source) = match load_contract(&canonical_root) {
        Ok(value) => value,
        Err(diagnostic) => return ValidationReport { diagnostics: vec![diagnostic] },
    };

    validate_contract_identity(&contract, &mut diagnostics);
    if !diagnostics.is_empty() {
        return validation_report(diagnostics);
    }
    validate_contract_paths(&contract, &mut diagnostics);
    if !diagnostics.is_empty() {
        return validation_report(diagnostics);
    }

    let scan_started_with = diagnostics.len();
    let scan_completed =
        validate_bounded_filesystem(&canonical_root, &contract, &contract_source, &mut diagnostics);
    if !scan_completed || diagnostics.len() != scan_started_with {
        return validation_report(diagnostics);
    }

    validate_root_entries(&canonical_root, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_required_root_shapes(&canonical_root, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_component_containers(&canonical_root, &contract, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_paths(&canonical_root, &contract, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_component_entries(&canonical_root, &contract, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    let mut cargo_inputs = validate_members(&canonical_root, &contract, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_adapters(&canonical_root, &contract, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    let mut resolved_graph = None;
    if diagnostics.is_empty() && cargo_inputs.len() == contract.members.len().saturating_add(1) {
        let lock_snapshot = read_required_cargo_input_snapshot(
            &canonical_root,
            "Cargo.lock",
            MAX_SCANNED_FILE_BYTES,
            &mut diagnostics,
        );
        let toolchain_snapshot = read_required_cargo_input_snapshot(
            &canonical_root,
            "rust-toolchain.toml",
            MAX_MANIFEST_BYTES,
            &mut diagnostics,
        );
        if let (Some(lock_snapshot), Some(toolchain_snapshot)) = (lock_snapshot, toolchain_snapshot)
        {
            cargo_inputs.extend([lock_snapshot, toolchain_snapshot]);
            read_optional_cargo_input_snapshots(
                &canonical_root,
                &mut cargo_inputs,
                &mut diagnostics,
            );
            if diagnostics.is_empty()
                && let Some(metadata) = load_cargo_metadata(&canonical_root, true, &mut diagnostics)
            {
                resolved_graph = Some(validate_resolved_cargo_graph(
                    &canonical_root,
                    &contract,
                    &metadata,
                    &mut diagnostics,
                ));
            }
        }
        if !diagnostics.is_halted() {
            validate_cargo_inputs_unchanged(&canonical_root, &cargo_inputs, &mut diagnostics);
        }
    }
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_dependency_graph(&contract, resolved_graph.as_ref(), &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_contract_unchanged(&canonical_root, &contract_source, &mut diagnostics);

    validation_report(diagnostics)
}
