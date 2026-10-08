use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use serde::Deserialize;
use std::path::Path;

pub(crate) const MAX_CARGO_EDGES: usize = 65_536;
pub(crate) const MAX_CARGO_PACKAGES: usize = 4096;
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CargoMetadataDocument {
    pub(crate) version: u32,
    pub(crate) packages: Vec<CargoMetadataPackage>,
    pub(crate) workspace_members: Vec<String>,
    pub(crate) workspace_root: String,
    pub(crate) resolve: Option<CargoMetadataResolve>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CargoMetadataPackage {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) manifest_path: String,
    pub(crate) source: Option<String>,
    #[serde(default)]
    pub(crate) dependencies: Vec<CargoMetadataDependency>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CargoMetadataDependency {
    pub(crate) name: String,
    pub(crate) rename: Option<String>,
    pub(crate) kind: Option<String>,
    pub(crate) target: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) optional: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CargoMetadataResolve {
    pub(crate) nodes: Vec<CargoMetadataNode>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CargoMetadataNode {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) deps: Vec<CargoMetadataNodeDependency>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CargoMetadataNodeDependency {
    pub(crate) name: String,
    pub(crate) pkg: String,
    #[serde(default)]
    pub(crate) dep_kinds: Vec<CargoMetadataDependencyKind>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CargoMetadataDependencyKind {
    pub(crate) kind: Option<String>,
    pub(crate) target: Option<String>,
}

pub(crate) fn validate_cargo_metadata_limits(
    metadata: &CargoMetadataDocument,
    diagnostics: &mut ValidationDiagnostics,
) -> bool {
    let edge_count = metadata
        .packages
        .iter()
        .map(|package| package.dependencies.len())
        .chain(
            metadata
                .resolve
                .iter()
                .flat_map(|resolve| resolve.nodes.iter().map(|node| node.deps.len())),
        )
        .fold(0_usize, usize::saturating_add);
    if metadata.packages.len() > MAX_CARGO_PACKAGES || edge_count > MAX_CARGO_EDGES {
        diagnostics.halt(architecture_error(
            "ZRYNA-A1204",
            Some(Path::new("Cargo.toml")),
            format!(
                "Cargo metadata contains {} packages and {edge_count} dependency edges",
                metadata.packages.len()
            ),
            format!(
                "keep the graph within {MAX_CARGO_PACKAGES} packages and {MAX_CARGO_EDGES} edges"
            ),
        ));
        return false;
    }
    true
}
