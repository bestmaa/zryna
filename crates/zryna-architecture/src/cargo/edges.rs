use crate::cargo::cargo_manifest_diagnostic_path;
use crate::cargo::metadata::CargoMetadataDependency;
use crate::cargo::metadata::CargoMetadataDocument;
use crate::cargo::metadata::CargoMetadataNode;
use crate::cargo::metadata::CargoMetadataNodeDependency;
use crate::cargo::sorted_metadata_packages;
use crate::contract::WorkspaceContract;
use crate::dependency_graph::InternalDependencyGraph;
use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn collect_declared_cargo_edges(
    root: &Path,
    metadata: &CargoMetadataDocument,
    registered_roots: &BTreeMap<PathBuf, String>,
    internal_packages: &BTreeMap<String, String>,
    graph: &mut InternalDependencyGraph,
    diagnostics: &mut ValidationDiagnostics,
) {
    for package in sorted_metadata_packages(metadata) {
        let Some(source_id) = internal_packages.get(&package.id) else {
            continue;
        };
        let diagnostic_path = cargo_manifest_diagnostic_path(root, package);
        let mut dependencies: Vec<&CargoMetadataDependency> = package.dependencies.iter().collect();
        dependencies.sort_by(|left, right| {
            (&left.name, &left.rename, &left.kind, &left.target, &left.path, left.optional).cmp(&(
                &right.name,
                &right.rename,
                &right.kind,
                &right.target,
                &right.path,
                right.optional,
            ))
        });
        for dependency in dependencies {
            if diagnostics.is_halted() {
                return;
            }
            if !valid_cargo_dependency_kind(dependency.kind.as_deref())
                || dependency.rename.as_deref().is_some_and(str::is_empty)
                || dependency.target.as_deref().is_some_and(str::is_empty)
            {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(&diagnostic_path),
                    format!("Cargo dependency '{}' has unsupported metadata", dependency.name),
                    "use normal, dev, or build dependency kinds with a valid target expression",
                ));
            }
            let Some(path) = &dependency.path else {
                continue;
            };
            let canonical_dependency = match fs::canonicalize(path) {
                Ok(value) => value,
                Err(error) => {
                    diagnostics.push(architecture_error(
                        "ZRYNA-A1101",
                        Some(&diagnostic_path),
                        format!(
                            "local dependency '{}' cannot be identified: {error}",
                            dependency.name
                        ),
                        "restore the dependency inside a registered member root",
                    ));
                    continue;
                }
            };
            let Some(target_id) = registered_roots.get(&canonical_dependency) else {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(&diagnostic_path),
                    format!(
                        "'{}' declares local dependency '{}' outside registered member roots",
                        source_id, dependency.name
                    ),
                    "register the local package or remove the dependency",
                ));
                continue;
            };
            if dependency.name != *target_id {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(&diagnostic_path),
                    format!(
                        "dependency metadata names '{}' but resolves registered member '{target_id}'",
                        dependency.name
                    ),
                    "bind every local dependency to its registered package identity",
                ));
            }
            graph.entry(source_id.clone()).or_default().insert(target_id.clone());
        }
    }
}

pub(crate) fn collect_resolved_cargo_edges(
    metadata: &CargoMetadataDocument,
    internal_packages: &BTreeMap<String, String>,
    expected_workspace: &BTreeSet<String>,
    graph: &mut InternalDependencyGraph,
    diagnostics: &mut ValidationDiagnostics,
) {
    let Some(resolve) = &metadata.resolve else {
        diagnostics.push(architecture_error(
            "ZRYNA-A1101",
            Some(Path::new("Cargo.toml")),
            "Cargo metadata omitted the resolved dependency graph",
            "run full metadata format version 1 without --no-deps",
        ));
        return;
    };
    let mut resolved_members = BTreeSet::new();
    let mut nodes: Vec<&CargoMetadataNode> = resolve.nodes.iter().collect();
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    for node in nodes {
        let Some(source_id) = internal_packages.get(&node.id) else {
            continue;
        };
        resolved_members.insert(source_id.clone());
        let mut dependencies: Vec<&CargoMetadataNodeDependency> = node.deps.iter().collect();
        dependencies.sort_by(|left, right| (&left.name, &left.pkg).cmp(&(&right.name, &right.pkg)));
        for dependency in dependencies {
            if dependency.name.is_empty()
                || dependency.dep_kinds.iter().any(|kind| {
                    !valid_cargo_dependency_kind(kind.kind.as_deref())
                        || kind.target.as_deref().is_some_and(str::is_empty)
                })
            {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(Path::new("Cargo.toml")),
                    format!("resolved dependency '{}' has unsupported metadata", dependency.name),
                    "use normal, dev, or build dependency kinds with a valid target expression",
                ));
            }
            if let Some(target_id) = internal_packages.get(&dependency.pkg) {
                graph.entry(source_id.clone()).or_default().insert(target_id.clone());
            }
        }
    }
    if &resolved_members != expected_workspace {
        diagnostics.push(architecture_error(
            "ZRYNA-A1101",
            Some(Path::new("Cargo.toml")),
            "resolved Cargo graph omits one or more registered members",
            "restore a complete full-workspace Cargo resolve graph",
        ));
    }
}

pub(crate) fn compare_internal_cargo_graph(
    contract: &WorkspaceContract,
    graph: &InternalDependencyGraph,
    diagnostics: &mut ValidationDiagnostics,
) {
    for member in &contract.members {
        let expected: BTreeSet<String> = member.dependencies.iter().cloned().collect();
        let actual = graph.get(&member.id).cloned().unwrap_or_default();
        if actual != expected {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(Path::new(&member.root).join("Cargo.toml").as_path()),
                format!(
                    "resolved internal dependencies for '{}' are {actual:?}, but the architecture contract declares {expected:?}",
                    member.id
                ),
                "declare the exact resolved normal, dev, build, target-specific, aliased, and patched internal graph",
            ));
        }
    }
}

fn valid_cargo_dependency_kind(kind: Option<&str>) -> bool {
    kind.is_none_or(|value| matches!(value, "dev" | "build"))
}
