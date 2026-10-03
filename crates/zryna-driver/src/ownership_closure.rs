//! Authenticated protocol-v4 module closure for the candidate ownership route.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::Path,
    time::{Duration, Instant},
};

use zryna_diagnostics::Diagnostic;
use zryna_frontend::{VerifiedFrontendProviderV4, syntax_v4};
use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap, UntrustedSpan};

use crate::{
    MAX_MODULE_DISCOVERY_ROUNDS, MAX_MODULE_DISCOVERY_WALL_TIME, MAX_MODULE_FILES,
    MAX_MODULE_IMPORT_DECLARATIONS, MAX_MODULE_IMPORT_EDGES, MAX_MODULE_PROVIDER_CALLS,
    MAX_MODULE_PROVIDER_SOURCE_BYTES, MAX_MODULE_SOURCE_BYTES, ModuleClosureError, ModuleEdge,
    ModuleRecord, WorkspaceSourceRoot,
    source_session::{ModuleSourceRoot, ModuleSourceSession},
};

const GRAPH_DOMAIN: &[u8] = b"ZRYNA-M3-GRAPH\0";

mod overlay;
mod support;
pub use overlay::discover_ownership_module_closure_with_overlays;
pub(crate) use overlay::discover_ownership_module_closure_with_overlays_bounded;
pub(crate) use support::{
    graph_identity as native_graph_identity, reject_cycles as validate_native_graph,
    seal_native_closure,
};
#[cfg(test)]
mod tests;
use support::{
    account_provider, budget, diagnostic, enforce_time_limit, final_edges, graph_identity, imports,
    imports_match, invalid_import, invariant, register_portable, reject_cycles,
    reject_provider_errors, rejected, remaining_with_limit,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct ImportBinding {
    imported: String,
    local: String,
    imported_span: UntrustedSpan,
    local_span: UntrustedSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Import {
    span: UntrustedSpan,
    specifier: String,
    specifier_span: UntrustedSpan,
    bindings: Vec<ImportBinding>,
}

struct DiscoveredSource {
    text: String,
    sha256: [u8; 32],
    imports: Vec<Import>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct EdgeIdentity {
    importer: NormalizedSourcePath,
    specifier: String,
    imported: String,
    local: String,
}

struct ClosureInputs {
    discovered: BTreeMap<NormalizedSourcePath, DiscoveredSource>,
    edge_ids: HashSet<EdgeIdentity>,
    aggregate_bytes: usize,
    provider_bytes: usize,
    provider_calls: usize,
}

/// Driver-owned protocol-v4 source closure used by the internal candidate route.
#[derive(Debug)]
pub struct VerifiedOwnershipModuleClosure {
    entrypoint: NormalizedSourcePath,
    sources: SourceMap,
    syntax: syntax_v4::ProjectSyntaxSnapshot,
    modules: Vec<ModuleRecord>,
    edges: Vec<ModuleEdge>,
    graph_sha256: [u8; 32],
}

impl VerifiedOwnershipModuleClosure {
    /// Returns the independently selected entry module.
    #[must_use]
    pub const fn entrypoint(&self) -> &NormalizedSourcePath {
        &self.entrypoint
    }
    /// Returns the immutable source authority authenticated by protocol v4.
    #[must_use]
    pub const fn sources(&self) -> &SourceMap {
        &self.sources
    }
    /// Returns the exact final protocol-v4 syntax authority.
    #[must_use]
    pub const fn syntax(&self) -> &syntax_v4::ProjectSyntaxSnapshot {
        &self.syntax
    }
    /// Returns canonical modules in normalized path order.
    #[must_use]
    pub fn modules(&self) -> &[ModuleRecord] {
        &self.modules
    }
    /// Returns canonical import edges.
    #[must_use]
    pub fn edges(&self) -> &[ModuleEdge] {
        &self.edges
    }
    /// Returns the profile-separated canonical source-graph digest.
    #[must_use]
    pub const fn graph_sha256(&self) -> &[u8; 32] {
        &self.graph_sha256
    }

    /// Lowers exactly this final authority through the mandatory ownership verifier.
    ///
    /// # Errors
    /// Returns stable diagnostics if the retained entry or source binding is inconsistent.
    pub fn lower_data_ownership_v1(
        &self,
    ) -> Result<zryna_semantics::data_ownership_v1::VerifiedProgram, Vec<Diagnostic>> {
        let Some(entry) = self.sources.file_id(&self.entrypoint) else {
            return Err(vec![diagnostic(
                "ZRYNA-D3302",
                Some(&self.entrypoint),
                "authenticated ownership closure lost its selected entry authority",
            )]);
        };
        let Some(input) = zryna_semantics::data_ownership_v1::SemanticInput::try_new(
            &self.syntax,
            &self.sources,
            entry,
        ) else {
            return Err(vec![diagnostic(
                "ZRYNA-D3302",
                Some(&self.entrypoint),
                "authenticated ownership closure cannot enter the exact M3 semantic boundary",
            )]);
        };
        zryna_semantics::data_ownership_v1::lower(input)
    }
}

/// Discovers one bounded module graph and retains only its final exact-v4 analysis.
///
/// Intermediate discovery snapshots are never compiler authority. All selected targets consume
/// the single final syntax/source authority returned here.
///
/// # Errors
/// Returns a fail-closed frontend or deterministic source-graph rejection before backend effects.
pub fn discover_ownership_module_closure<Provider: VerifiedFrontendProviderV4 + ?Sized>(
    root: &WorkspaceSourceRoot,
    entrypoint: NormalizedSourcePath,
    frontend: &Provider,
) -> Result<VerifiedOwnershipModuleClosure, ModuleClosureError> {
    discover_with_clock(root, entrypoint, frontend, Instant::now)
}

pub(crate) fn discover_with_clock<Provider, Clock>(
    root: &impl ModuleSourceRoot,
    entrypoint: NormalizedSourcePath,
    frontend: &Provider,
    now: Clock,
) -> Result<VerifiedOwnershipModuleClosure, ModuleClosureError>
where
    Provider: VerifiedFrontendProviderV4 + ?Sized,
    Clock: FnMut() -> Instant,
{
    discover_with_clock_limit(root, entrypoint, frontend, now, MAX_MODULE_DISCOVERY_WALL_TIME)
}

pub(crate) fn discover_with_clock_limit<Provider, Clock>(
    root: &impl ModuleSourceRoot,
    entrypoint: NormalizedSourcePath,
    frontend: &Provider,
    mut now: Clock,
    limit: Duration,
) -> Result<VerifiedOwnershipModuleClosure, ModuleClosureError>
where
    Provider: VerifiedFrontendProviderV4 + ?Sized,
    Clock: FnMut() -> Instant,
{
    let started = now();
    if Path::new(entrypoint.as_str()).extension().and_then(|value| value.to_str()) != Some("zry") {
        return Err(rejected(diagnostic(
            "ZRYNA-D3301",
            Some(&entrypoint),
            "entry module must use the exact lowercase .zry extension",
        )));
    }
    let mut session = root.begin().map_err(rejected)?;
    let mut discovered = BTreeMap::<NormalizedSourcePath, DiscoveredSource>::new();
    let mut portable = BTreeMap::from([(entrypoint.portable_identity(), entrypoint.clone())]);
    let mut pending = BTreeSet::from([entrypoint.clone()]);
    let mut edge_ids = HashSet::new();
    let mut aggregate_bytes = 0_usize;
    let mut provider_bytes = 0_usize;
    let mut provider_calls = 0_usize;
    let mut rounds = 0_usize;
    let mut import_count = 0_usize;

    while !pending.is_empty() {
        rounds = add(rounds, 1)?;
        if rounds > MAX_MODULE_DISCOVERY_ROUNDS
            || discovered.len().checked_add(pending.len()).is_none_or(|n| n > MAX_MODULE_FILES)
        {
            return Err(budget("ownership module discovery exceeded its graph budget"));
        }
        let paths = std::mem::take(&mut pending);
        let mut stable = BTreeMap::new();
        let mut inputs = Vec::with_capacity(paths.len());
        let mut batch_bytes = 0_usize;
        for path in paths {
            let source = session.read_source(&path).map_err(rejected)?;
            batch_bytes = add(batch_bytes, source.text.len())?;
            aggregate_bytes = add(aggregate_bytes, source.text.len())?;
            if aggregate_bytes > MAX_MODULE_SOURCE_BYTES {
                return Err(budget("ownership module discovery exceeded its source-byte budget"));
            }
            inputs.push(SourceFileInput {
                path: path.as_str().to_owned(),
                text: source.text.clone(),
            });
            stable.insert(path, source);
        }
        account_provider(&mut provider_calls, &mut provider_bytes, batch_bytes, false)?;
        let source_map = SourceMap::build(inputs).map_err(|_| invariant())?;
        session.validate_provider_batch(&source_map).map_err(rejected)?;
        let snapshot = frontend
            .analyze_verified_v4_with_timeout(
                &source_map,
                remaining_with_limit(started, now(), frontend.minimum_analysis_timeout(), limit)?,
            )
            .map_err(ModuleClosureError::Frontend)?;
        enforce_time_limit(started, now(), limit)?;
        reject_provider_errors(&snapshot)?;
        let imports = imports(&snapshot);
        for (path, source) in stable {
            let file_imports = imports.get(&path).cloned().ok_or_else(invariant)?;
            for import in &file_imports {
                import_count = add(import_count, 1)?;
                if import_count > MAX_MODULE_IMPORT_DECLARATIONS {
                    return Err(budget("ownership module discovery exceeded its import budget"));
                }
                let target = zryna_source::resolve_explicit_zry_import(&path, &import.specifier)
                    .map_err(|_| invalid_import(&path))?;
                register_portable(&mut portable, &target)?;
                register_import_edges(&mut edge_ids, &path, import)?;
                if !discovered.contains_key(&target) && !imports.contains_key(&target) {
                    pending.insert(target);
                }
            }
            discovered.insert(
                path,
                DiscoveredSource {
                    text: source.text,
                    sha256: source.sha256,
                    imports: file_imports,
                },
            );
        }
    }

    finalize_closure(
        &mut session,
        entrypoint,
        frontend,
        started,
        &mut now,
        ClosureInputs { discovered, edge_ids, aggregate_bytes, provider_bytes, provider_calls },
        limit,
    )
}

fn finalize_closure<Provider, Clock>(
    session: &mut impl ModuleSourceSession,
    entrypoint: NormalizedSourcePath,
    frontend: &Provider,
    started: Instant,
    now: &mut Clock,
    mut inputs: ClosureInputs,
    limit: Duration,
) -> Result<VerifiedOwnershipModuleClosure, ModuleClosureError>
where
    Provider: VerifiedFrontendProviderV4 + ?Sized,
    Clock: FnMut() -> Instant,
{
    session.revalidate_all().map_err(rejected)?;
    let final_inputs = inputs
        .discovered
        .iter()
        .map(|(path, source)| SourceFileInput {
            path: path.as_str().to_owned(),
            text: source.text.clone(),
        })
        .collect();
    account_provider(
        &mut inputs.provider_calls,
        &mut inputs.provider_bytes,
        inputs.aggregate_bytes,
        true,
    )?;
    let sources = SourceMap::build(final_inputs).map_err(|_| invariant())?;
    let syntax = frontend
        .analyze_verified_v4_with_timeout(
            &sources,
            remaining_with_limit(started, now(), frontend.minimum_analysis_timeout(), limit)?,
        )
        .map_err(ModuleClosureError::Frontend)?;
    enforce_time_limit(started, now(), limit)?;
    reject_provider_errors(&syntax)?;
    session.revalidate_all().map_err(rejected)?;
    if !syntax.is_bound_to(&sources) {
        return Err(invariant());
    }
    let final_imports = imports(&syntax);
    if !imports_match(&final_imports, &inputs.discovered) {
        return Err(rejected(diagnostic(
            "ZRYNA-D3302",
            None,
            "final authenticated ownership imports differ from fixed-point discovery",
        )));
    }
    let modules = inputs
        .discovered
        .iter()
        .enumerate()
        .map(|(index, (path, source))| {
            Ok(ModuleRecord {
                id: u32::try_from(index).map_err(|_| invariant())?,
                path: path.clone(),
                source_sha256: source.sha256,
            })
        })
        .collect::<Result<Vec<_>, ModuleClosureError>>()?;
    let edges = final_edges(&syntax, &sources)?;
    let actual = edges
        .iter()
        .map(|edge| EdgeIdentity {
            importer: edge.importer.clone(),
            specifier: edge.specifier.clone(),
            imported: edge.imported.clone(),
            local: edge.local.clone(),
        })
        .collect::<HashSet<_>>();
    if actual != inputs.edge_ids {
        return Err(invariant());
    }
    reject_cycles(&modules, &edges)?;
    let graph_sha256 = graph_identity(&entrypoint, &modules, &edges)?;
    Ok(VerifiedOwnershipModuleClosure { entrypoint, sources, syntax, modules, edges, graph_sha256 })
}

fn register_import_edges(
    edges: &mut HashSet<EdgeIdentity>,
    path: &NormalizedSourcePath,
    import: &Import,
) -> Result<(), ModuleClosureError> {
    for binding in &import.bindings {
        let identity = EdgeIdentity {
            importer: path.clone(),
            specifier: import.specifier.clone(),
            imported: binding.imported.clone(),
            local: binding.local.clone(),
        };
        if edges.len() >= MAX_MODULE_IMPORT_EDGES || !edges.insert(identity) {
            return Err(rejected(diagnostic(
                "ZRYNA-D3301",
                Some(path),
                "duplicate or excessive ownership import edge",
            )));
        }
    }
    Ok(())
}

fn add(left: usize, right: usize) -> Result<usize, ModuleClosureError> {
    left.checked_add(right).ok_or_else(|| budget("ownership module accounting overflowed"))
}
