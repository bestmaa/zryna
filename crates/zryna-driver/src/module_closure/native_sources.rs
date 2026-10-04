//! Native discovery owns retained source capabilities, never ambient provider paths.

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

use sha2::{Digest as _, Sha256};
use zryna_source::{
    NormalizedSourcePath, SourceFileInput, SourceMap, SourceMapIdentity,
    resolve_explicit_zry_import,
};

use super::{
    MAX_MODULE_FILES, MAX_MODULE_SOURCE_BYTES, ModuleClosureError, ModuleRecord,
    VerifiedModuleClosure, budget_rejection, checked_add, enforce_discovery_wall_time, final_edges,
    graph_identity, invariant_rejection, register_portable_path, rejected,
};
use crate::{
    AuthenticatedPackageSources, PackageLockMode, PackageResolutionRequest,
    package_resolution::CapturedProject,
    workspace_source::{StableSource, WorkspaceSourceRoot, WorkspaceSourceSession},
};

mod graph;
mod verification;
pub use verification::{NativeModuleSnapshot, NativeOwnershipSnapshot, NativeSyntaxSnapshot};
#[cfg(test)]
mod tests;

enum SourceOwner<'root> {
    Workspace(RefCell<WorkspaceSourceSession<'root>>),
    Package {
        capture: Box<CapturedProject>,
        sources: AuthenticatedPackageSources,
        lock_sha256: String,
    },
}

impl SourceOwner<'_> {
    fn read(&self, path: &NormalizedSourcePath) -> Result<StableSource, ModuleClosureError> {
        match self {
            Self::Workspace(session) => session
                .try_borrow_mut()
                .map_err(|_| invariant_rejection())?
                .read_source(path)
                .map_err(rejected),
            Self::Package { sources, .. } => {
                let file =
                    sources.files().iter().find(|file| file.path() == path.as_str()).ok_or_else(
                        || {
                            graph::failure(
                                "ZRYNA-P4004",
                                "source is absent from the admitted package inventory",
                            )
                        },
                    )?;
                let sha256: [u8; 32] = Sha256::digest(file.bytes()).into();
                if format!("{:x}", Sha256::digest(file.bytes())) != file.sha256() {
                    return Err(graph::failure(
                        "ZRYNA-P4004",
                        "admitted package source hash differs",
                    ));
                }
                let text = String::from_utf8(file.bytes().to_vec())
                    .map_err(|_| graph::failure("ZRYNA-D3003", "source is not valid UTF-8"))?;
                Ok(StableSource { text, sha256 })
            }
        }
    }

    fn revalidate(&self) -> Result<(), ModuleClosureError> {
        match self {
            Self::Workspace(session) => session
                .try_borrow_mut()
                .map_err(|_| invariant_rejection())?
                .revalidate_all()
                .map_err(rejected),
            Self::Package { capture, .. } => capture.revalidate().map_err(|error| {
                graph::failure(error.code(), "retained package capability changed")
            }),
        }
    }
}

/// Exact immutable native source closure sealed before complete syntax parsing.
///
/// Retains original UTF-8 text, canonical paths, raw SHA-256, import edges, graph identities and
/// the issuing source map together with the no-follow owner. This internal route selects no
/// public provider and has no semantic or target authority until versioned verification succeeds.
pub struct NativeSourceSnapshot<'root> {
    owner: SourceOwner<'root>,
    entrypoint: NormalizedSourcePath,
    sources: SourceMap,
    source_identity: SourceMapIdentity,
    modules: Vec<ModuleRecord>,
    edges: Vec<super::ModuleEdge>,
    graph_v3: [u8; 32],
    graph_v4: [u8; 32],
}

impl NativeSourceSnapshot<'_> {
    /// Returns the original immutable source authority used by later parsing and verification.
    #[must_use]
    pub const fn sources(&self) -> &SourceMap {
        &self.sources
    }

    /// Returns exact source hashes in canonical path order.
    #[must_use]
    pub fn modules(&self) -> &[ModuleRecord] {
        &self.modules
    }

    /// Returns the canonical v3 graph digest before complete syntax parsing.
    #[must_use]
    pub const fn graph_sha256_v3(&self) -> &[u8; 32] {
        &self.graph_v3
    }

    /// Returns the canonical v4 graph digest before complete syntax parsing.
    #[must_use]
    pub const fn graph_sha256_v4(&self) -> &[u8; 32] {
        &self.graph_v4
    }

    /// Returns the exact admitted manifest/source pair and lock digest for package snapshots.
    #[must_use]
    pub fn package_identity(&self) -> Option<(&str, &str, &str)> {
        match &self.owner {
            SourceOwner::Workspace(_) => None,
            SourceOwner::Package { sources, lock_sha256, .. } => {
                Some((sources.package_id(), sources.source_sha256(), lock_sha256))
            }
        }
    }

    /// Rechecks retained identities and immutable hashes without reopening source content.
    ///
    /// # Errors
    /// Rejects stale source/directory capabilities or inconsistent sealed graph records.
    pub fn revalidate(&self) -> Result<(), ModuleClosureError> {
        self.owner.revalidate()?;
        graph::authenticate(self)
    }
}

/// Captures a bounded native closure below one retained workspace capability.
///
/// # Errors
/// Rejects unsafe paths, malformed imports, links, collisions, cycles, stale handles and bounds.
pub fn capture_native_workspace_sources(
    root: &WorkspaceSourceRoot,
    entrypoint: NormalizedSourcePath,
) -> Result<NativeSourceSnapshot<'_>, ModuleClosureError> {
    let owner = SourceOwner::Workspace(RefCell::new(root.begin_discovery().map_err(rejected)?));
    capture_sources(owner, entrypoint)
}

/// Captures internal relative imports within one exact instance of an already admitted graph.
///
/// Resolution must be frozen; this operation performs no source update or lock publication.
/// Dependency aliases do not introduce cross-package import syntax or ambient search authority.
///
/// # Errors
/// Rejects non-frozen requests, absent exact package identities and every package/source failure.
pub fn capture_native_package_sources(
    request: &PackageResolutionRequest,
    package_id: &str,
    entrypoint: NormalizedSourcePath,
) -> Result<NativeSourceSnapshot<'static>, ModuleClosureError> {
    if request.mode != PackageLockMode::Frozen {
        return Err(graph::failure(
            "ZRYNA-P4010",
            "native source capture requires frozen package resolution",
        ));
    }
    let (resolved, capture) = crate::package_resolution::capture_package(request)
        .map_err(|error| graph::failure(error.code(), "frozen package source admission failed"))?;
    let sources = resolved
        .sources()
        .iter()
        .find(|sources| sources.package_id() == package_id)
        .cloned()
        .ok_or_else(|| graph::failure("ZRYNA-P4006", "exact package identity is not admitted"))?;
    let owner = SourceOwner::Package {
        capture: Box::new(capture),
        sources,
        lock_sha256: resolved.graph().lock_sha256().to_owned(),
    };
    capture_sources(owner, entrypoint)
}

fn capture_sources(
    owner: SourceOwner<'_>,
    entrypoint: NormalizedSourcePath,
) -> Result<NativeSourceSnapshot<'_>, ModuleClosureError> {
    let started = Instant::now();
    if !super::has_exact_zry_extension(entrypoint.as_str()) {
        return Err(graph::failure("ZRYNA-D3001", "entry must use the exact .zry extension"));
    }
    let mut found = BTreeMap::new();
    let mut portable = BTreeMap::from([(entrypoint.portable_identity(), entrypoint.clone())]);
    let mut pending = BTreeSet::from([entrypoint.clone()]);
    let mut bytes = 0;
    while let Some(path) = pending.pop_first() {
        if found.len() >= MAX_MODULE_FILES {
            return Err(budget_rejection("native source discovery exceeded the file budget"));
        }
        let stable = owner.read(&path)?;
        bytes = checked_add(bytes, stable.text.len())?;
        if bytes > MAX_MODULE_SOURCE_BYTES {
            return Err(budget_rejection(
                "native source discovery exceeded the source-byte budget",
            ));
        }
        let batch = SourceMap::build(vec![SourceFileInput {
            path: path.as_str().to_owned(),
            text: stable.text.clone(),
        }])
        .map_err(|_| invariant_rejection())?;
        let imports = graph::discover(&batch)?;
        found.insert(path.clone(), stable);
        for import in &imports[0].imports {
            let target = resolve_explicit_zry_import(&path, &import.specifier.text)
                .map_err(|_| super::invalid_specifier(&path))?;
            register_portable_path(&mut portable, &target)?;
            if !found.contains_key(&target) {
                pending.insert(target);
            }
            if found.len().checked_add(pending.len()).is_none_or(|n| n > MAX_MODULE_FILES) {
                return Err(budget_rejection("native source discovery exceeded the file budget"));
            }
        }
        enforce_discovery_wall_time(started, Instant::now())?;
    }
    owner.revalidate()?;
    let sources = SourceMap::build(
        found
            .iter()
            .map(|(path, source)| SourceFileInput {
                path: path.as_str().to_owned(),
                text: source.text.clone(),
            })
            .collect(),
    )
    .map_err(|_| invariant_rejection())?;
    let modules = found
        .into_iter()
        .enumerate()
        .map(|(id, (path, stable))| {
            Ok(ModuleRecord {
                id: u32::try_from(id).map_err(|_| invariant_rejection())?,
                path,
                source_sha256: stable.sha256,
            })
        })
        .collect::<Result<Vec<_>, ModuleClosureError>>()?;
    let edges = graph::candidate_edges(&sources)?;
    let graph_v3 = graph_identity(&entrypoint, &modules, &edges)?;
    let graph_v4 = crate::ownership_closure::native_graph_identity(&entrypoint, &modules, &edges)?;
    let source_identity = sources.identity();
    let snapshot = NativeSourceSnapshot {
        owner,
        entrypoint,
        sources,
        source_identity,
        modules,
        edges,
        graph_v3,
        graph_v4,
    };
    snapshot.revalidate()?;
    enforce_discovery_wall_time(started, Instant::now())?;
    Ok(snapshot)
}
