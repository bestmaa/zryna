pub(super) mod capture;
pub(super) mod edges;
pub(super) mod inputs;
pub(super) mod metadata;
pub(super) mod process;

use crate::cargo::edges::collect_declared_cargo_edges;
use crate::cargo::edges::collect_resolved_cargo_edges;
use crate::cargo::edges::compare_internal_cargo_graph;
use crate::cargo::metadata::CargoMetadataDocument;
use crate::cargo::metadata::CargoMetadataPackage;
use crate::contract::WorkspaceContract;
use crate::dependency_graph::InternalDependencyGraph;
use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn validate_resolved_cargo_graph(
    root: &Path,
    contract: &WorkspaceContract,
    metadata: &CargoMetadataDocument,
    diagnostics: &mut ValidationDiagnostics,
) -> InternalDependencyGraph {
    validate_metadata_workspace_root(root, metadata, diagnostics);
    let registered_roots = registered_cargo_roots(root, contract, diagnostics);
    let internal_packages =
        map_internal_cargo_packages(root, metadata, &registered_roots, diagnostics);
    let expected_workspace: BTreeSet<String> =
        contract.members.iter().map(|member| member.id.clone()).collect();
    validate_metadata_workspace_members(
        metadata,
        &internal_packages,
        &expected_workspace,
        diagnostics,
    );
    let mut graph: InternalDependencyGraph =
        contract.members.iter().map(|member| (member.id.clone(), BTreeSet::new())).collect();
    collect_declared_cargo_edges(
        root,
        metadata,
        &registered_roots,
        &internal_packages,
        &mut graph,
        diagnostics,
    );
    collect_resolved_cargo_edges(
        metadata,
        &internal_packages,
        &expected_workspace,
        &mut graph,
        diagnostics,
    );
    compare_internal_cargo_graph(contract, &graph, diagnostics);
    graph
}

fn validate_metadata_workspace_root(
    root: &Path,
    metadata: &CargoMetadataDocument,
    diagnostics: &mut ValidationDiagnostics,
) {
    let controlled_root = match fs::canonicalize(root) {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(Path::new("Cargo.toml")),
                format!("controlled workspace root cannot be identified: {error}"),
                "restore the canonical controlled workspace root",
            ));
            return;
        }
    };
    match fs::canonicalize(&metadata.workspace_root) {
        Ok(workspace_root) if workspace_root == controlled_root => {}
        Ok(workspace_root) => diagnostics.push(architecture_error(
            "ZRYNA-A1101",
            Some(Path::new("Cargo.toml")),
            format!(
                "Cargo metadata resolved workspace root '{}' instead of the controlled root",
                workspace_root.display()
            ),
            "run metadata only for the canonical controlled workspace",
        )),
        Err(error) => diagnostics.push(architecture_error(
            "ZRYNA-A1101",
            Some(Path::new("Cargo.toml")),
            format!("Cargo metadata workspace root cannot be identified: {error}"),
            "restore the canonical controlled workspace root",
        )),
    }
}

fn registered_cargo_roots(
    root: &Path,
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) -> BTreeMap<PathBuf, String> {
    let mut registered_roots = BTreeMap::new();
    for member in &contract.members {
        if diagnostics.is_halted() {
            return registered_roots;
        }
        match fs::canonicalize(root.join(&member.root)) {
            Ok(path) => {
                registered_roots.insert(path, member.id.clone());
            }
            Err(error) => diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new(&member.root)),
                format!("registered Cargo member root cannot be identified: {error}"),
                "restore the registered member directory",
            )),
        }
    }
    registered_roots
}

fn map_internal_cargo_packages(
    root: &Path,
    metadata: &CargoMetadataDocument,
    registered_roots: &BTreeMap<PathBuf, String>,
    diagnostics: &mut ValidationDiagnostics,
) -> BTreeMap<String, String> {
    let mut internal_packages = BTreeMap::new();
    let mut package_roots = BTreeMap::new();
    for package in sorted_metadata_packages(metadata) {
        if diagnostics.is_halted() {
            return internal_packages;
        }
        if package.source.is_some() {
            continue;
        }
        let manifest = PathBuf::from(&package.manifest_path);
        let canonical_manifest = match fs::canonicalize(&manifest) {
            Ok(value) => value,
            Err(error) => {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(Path::new("Cargo.toml")),
                    format!("resolved local Cargo manifest cannot be identified: {error}"),
                    "restore every resolved local package inside a registered member root",
                ));
                continue;
            }
        };
        let Some(package_root) = canonical_manifest.parent() else {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(Path::new("Cargo.toml")),
                "resolved local Cargo manifest has no package root",
                "restore every resolved local package inside a registered member root",
            ));
            continue;
        };
        let Some(member_id) = registered_roots.get(package_root) else {
            let diagnostic_path = canonical_manifest
                .strip_prefix(root)
                .map_or_else(|_| PathBuf::from("Cargo.toml"), Path::to_path_buf);
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(&diagnostic_path),
                format!("Cargo resolves unregistered local package '{}'", package.name),
                "register the package as one canonical member or remove the local dependency",
            ));
            continue;
        };
        let diagnostic_path = cargo_manifest_diagnostic_path(root, package);
        if canonical_manifest != package_root.join("Cargo.toml") {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(&diagnostic_path),
                "Cargo metadata manifest path is not the exact registered Cargo.toml",
                "bind the package id to the exact snapshotted member manifest",
            ));
        }
        if package.name != *member_id {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(&diagnostic_path),
                format!(
                    "resolved package '{}' does not match registered member '{member_id}'",
                    package.name
                ),
                "make the package name, component id, and canonical root identical",
            ));
        }
        if internal_packages.insert(package.id.clone(), member_id.clone()).is_some()
            || package_roots.insert(package_root.to_path_buf(), package.id.clone()).is_some()
        {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(&diagnostic_path),
                "Cargo metadata contains a duplicate local package identity",
                "keep exactly one package id for each registered component root",
            ));
        }
    }
    internal_packages
}

fn validate_metadata_workspace_members(
    metadata: &CargoMetadataDocument,
    internal_packages: &BTreeMap<String, String>,
    expected_workspace: &BTreeSet<String>,
    diagnostics: &mut ValidationDiagnostics,
) {
    let mut actual_workspace = BTreeSet::new();
    for package_id in &metadata.workspace_members {
        if let Some(member_id) = internal_packages.get(package_id) {
            actual_workspace.insert(member_id.clone());
        } else {
            diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new("Cargo.toml")),
                format!("Cargo workspace contains unregistered package id '{package_id}'"),
                "make Cargo workspace_members and zryna.workspace.json identical",
            ));
        }
    }
    if &actual_workspace != expected_workspace
        || metadata.workspace_members.len() != expected_workspace.len()
    {
        diagnostics.push(architecture_error(
            "ZRYNA-A1005",
            Some(Path::new("Cargo.toml")),
            "resolved Cargo workspace members differ from zryna.workspace.json",
            "register every resolved workspace package exactly once",
        ));
    }
}

pub(crate) fn sorted_metadata_packages(
    metadata: &CargoMetadataDocument,
) -> Vec<&CargoMetadataPackage> {
    let mut packages: Vec<&CargoMetadataPackage> = metadata.packages.iter().collect();
    packages.sort_by(|left, right| left.id.cmp(&right.id));
    packages
}

pub(crate) fn cargo_manifest_diagnostic_path(
    root: &Path,
    package: &CargoMetadataPackage,
) -> PathBuf {
    fs::canonicalize(&package.manifest_path)
        .ok()
        .and_then(|path| path.strip_prefix(root).map(Path::to_path_buf).ok())
        .unwrap_or_else(|| PathBuf::from("Cargo.toml"))
}
