use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    error::Error,
    fmt,
    time::{Duration, Instant},
};

use zryna_diagnostics::{Diagnostic, Severity};
use zryna_frontend::{WorkerError, syntax_v3};
use zryna_source::{
    NormalizedSourcePath, SourceFileInput, SourceMap, Span, resolve_explicit_zry_import,
};

use crate::source_session::{ModuleSourceRoot, ModuleSourceSession};
use crate::workspace_source::{MAX_DIRECTORY_ENTRIES, StableSource};
pub(crate) mod entry;
pub(crate) mod native_sources;
pub use entry::{discover_module_closure, discover_native_import_only_closure};

/// Maximum modules in one M2 closure.
pub const MAX_MODULE_FILES: usize = 4_096;
/// Maximum aggregate source bytes in one M2 closure.
pub const MAX_MODULE_SOURCE_BYTES: usize = 8 * 1_024 * 1_024;
/// Maximum fixed-point discovery rounds.
pub const MAX_MODULE_DISCOVERY_ROUNDS: usize = 4_096;
/// Maximum provider calls including the one final full-map call.
pub const MAX_MODULE_PROVIDER_CALLS: usize = 4_097;
/// Maximum aggregate wall-clock duration of one module-closure discovery operation.
pub const MAX_MODULE_DISCOVERY_WALL_TIME: Duration = Duration::from_mins(2);
/// Maximum cumulative source bytes supplied to the provider across discovery and final analysis.
pub const MAX_MODULE_PROVIDER_SOURCE_BYTES: usize = 16 * 1_024 * 1_024;
/// Maximum canonical named-import binding edges.
pub const MAX_MODULE_IMPORT_EDGES: usize = 65_536;
/// Maximum conservative canonical manifest bytes attributable to named-import edges.
pub const MAX_MODULE_EDGE_MANIFEST_BYTES: usize = 32 * 1_024 * 1_024;
/// Maximum import declarations across the complete closure.
pub const MAX_MODULE_IMPORT_DECLARATIONS: usize = 65_536;
/// Maximum entries inspected in any retained source directory.
pub const MAX_MODULE_DIRECTORY_ENTRIES: usize = MAX_DIRECTORY_ENTRIES;

const GRAPH_DOMAIN: &[u8] = b"ZRYNA-M2-GRAPH\0";

/// One canonical module identity in normalized path order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleRecord {
    pub(crate) id: u32,
    pub(crate) path: NormalizedSourcePath,
    pub(crate) source_sha256: [u8; 32],
}

impl ModuleRecord {
    /// Returns the dense module identifier assigned by normalized path byte order.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the normalized portable module path.
    #[must_use]
    pub const fn path(&self) -> &NormalizedSourcePath {
        &self.path
    }

    /// Returns SHA-256 over the exact UTF-8 source bytes.
    #[must_use]
    pub const fn source_sha256(&self) -> &[u8; 32] {
        &self.source_sha256
    }
}

/// One canonical named-import binding edge carrying only final-map spans.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleEdge {
    pub(crate) importer: NormalizedSourcePath,
    pub(crate) target: NormalizedSourcePath,
    pub(crate) specifier: String,
    pub(crate) imported: String,
    pub(crate) local: String,
    pub(crate) declaration_span: Span,
    pub(crate) specifier_span: Span,
    pub(crate) imported_span: Span,
    pub(crate) local_span: Span,
}

impl ModuleEdge {
    /// Returns the importing module.
    #[must_use]
    pub const fn importer(&self) -> &NormalizedSourcePath {
        &self.importer
    }

    /// Returns the driver-resolved dependency module.
    #[must_use]
    pub const fn target(&self) -> &NormalizedSourcePath {
        &self.target
    }

    /// Returns the exact verified source specifier.
    #[must_use]
    pub fn specifier(&self) -> &str {
        &self.specifier
    }

    /// Returns the imported source name.
    #[must_use]
    pub fn imported(&self) -> &str {
        &self.imported
    }

    /// Returns the local binding name.
    #[must_use]
    pub fn local(&self) -> &str {
        &self.local
    }

    /// Returns the complete import declaration span in the final source map.
    #[must_use]
    pub const fn declaration_span(&self) -> Span {
        self.declaration_span
    }

    /// Returns the quoted module-specifier span in the final source map.
    #[must_use]
    pub const fn specifier_span(&self) -> Span {
        self.specifier_span
    }

    /// Returns the imported-name span in the final source map.
    #[must_use]
    pub const fn imported_span(&self) -> Span {
        self.imported_span
    }

    /// Returns the local-name span in the final source map.
    #[must_use]
    pub const fn local_span(&self) -> Span {
        self.local_span
    }
}

/// Immutable driver-owned module closure authenticated against one final source map.
#[derive(Debug)]
pub struct VerifiedModuleClosure {
    entrypoint: NormalizedSourcePath,
    sources: SourceMap,
    syntax: syntax_v3::ProjectSyntaxSnapshot,
    modules: Vec<ModuleRecord>,
    edges: Vec<ModuleEdge>,
    graph_sha256: [u8; 32],
}

impl VerifiedModuleClosure {
    /// Returns the entry module.
    #[must_use]
    pub const fn entrypoint(&self) -> &NormalizedSourcePath {
        &self.entrypoint
    }

    /// Returns the single final immutable source authority.
    #[must_use]
    pub const fn sources(&self) -> &SourceMap {
        &self.sources
    }

    /// Returns the exact-v3 syntax authenticated against [`Self::sources`].
    #[must_use]
    pub const fn syntax(&self) -> &syntax_v3::ProjectSyntaxSnapshot {
        &self.syntax
    }

    /// Returns canonical modules in dense path order.
    #[must_use]
    pub fn modules(&self) -> &[ModuleRecord] {
        &self.modules
    }

    /// Returns canonical named-binding edges.
    #[must_use]
    pub fn edges(&self) -> &[ModuleEdge] {
        &self.edges
    }

    /// Returns SHA-256 over the frozen canonical graph byte document.
    #[must_use]
    pub const fn graph_sha256(&self) -> &[u8; 32] {
        &self.graph_sha256
    }

    /// Runs the isolated straight-line M2 semantic boundary over this exact final closure.
    ///
    /// This does not enable a public compiler profile or backend. Success returns only mandatory
    /// `ControlFlowV1` verifier authority; raw semantic IR is never exposed.
    ///
    /// # Errors
    ///
    /// Returns deterministic semantic or IR diagnostics. A closure/input mismatch is reported as
    /// a driver invariant failure rather than falling back to a discovery snapshot.
    pub fn lower_control_flow_v1(
        &self,
    ) -> Result<zryna_ir::control_flow_v1::VerifiedProgram, Vec<Diagnostic>> {
        let Some(entry) = self.sources.file_id(&self.entrypoint) else {
            return Err(vec![module_diagnostic(
                "ZRYNA-D3202",
                Some(&self.entrypoint),
                "authenticated module closure lost its selected entry authority",
                "report this compiler invariant failure with the smallest reproducible workspace",
            )]);
        };
        let Some(input) = zryna_semantics::control_flow_v1::SemanticInput::try_new(
            &self.syntax,
            &self.sources,
            entry,
        ) else {
            return Err(vec![module_diagnostic(
                "ZRYNA-D3202",
                Some(&self.entrypoint),
                "authenticated module closure cannot enter the exact M2 semantic boundary",
                "report this compiler invariant failure with the smallest reproducible workspace",
            )]);
        };
        zryna_semantics::control_flow_v1::lower(input)
    }
}

/// Failure before a complete module closure can become compiler authority.
#[derive(Debug)]
pub enum ModuleClosureError {
    /// The authenticated exact-v3 frontend failed.
    Frontend(WorkerError),
    /// Driver resolution, filesystem, graph, or budget validation failed.
    Rejected(Vec<Diagnostic>),
}

impl ModuleClosureError {
    /// Returns stable diagnostics when available.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::Frontend(error) => error.diagnostics(),
            Self::Rejected(diagnostics) => diagnostics,
        }
    }
}

impl fmt::Display for ModuleClosureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frontend(error) => error.fmt(formatter),
            Self::Rejected(diagnostics) => write!(
                formatter,
                "module closure was rejected by {} deterministic diagnostic(s)",
                diagnostics.len()
            ),
        }
    }
}

impl Error for ModuleClosureError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Frontend(error) => Some(error),
            Self::Rejected(_) => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SpanFingerprint {
    start: u32,
    end: u32,
}

impl From<Span> for SpanFingerprint {
    fn from(span: Span) -> Self {
        Self { start: span.start(), end: span.end() }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BindingFingerprint {
    span: SpanFingerprint,
    imported: String,
    imported_span: SpanFingerprint,
    local: String,
    local_span: SpanFingerprint,
    as_span: Option<SpanFingerprint>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ImportFingerprint {
    span: SpanFingerprint,
    import_span: SpanFingerprint,
    bindings: Vec<BindingFingerprint>,
    from_span: SpanFingerprint,
    specifier: String,
    specifier_token_span: SpanFingerprint,
    specifier_value_span: SpanFingerprint,
    semicolon_span: SpanFingerprint,
}

struct DiscoveredSource {
    stable: StableSource,
    imports: Vec<ImportFingerprint>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct EdgeIdentity {
    importer: NormalizedSourcePath,
    specifier: String,
    imported: String,
    local: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ResolvedEdge {
    identity: EdgeIdentity,
    target: NormalizedSourcePath,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn discover_module_closure_with_clock<
    Provider: entry::ClosureFrontendV3 + ?Sized,
    Clock: FnMut() -> Instant,
>(
    root: &impl ModuleSourceRoot,
    entrypoint: NormalizedSourcePath,
    frontend: &Provider,
    mut now: Clock,
) -> Result<VerifiedModuleClosure, ModuleClosureError> {
    let discovery_started = now();
    if !has_exact_zry_extension(entrypoint.as_str()) {
        return Err(rejected(module_diagnostic(
            "ZRYNA-D3001",
            Some(&entrypoint),
            "entry module must use the exact lowercase .zry extension",
            "select one normalized portable .zry entry module",
        )));
    }
    let mut source_session = root.begin().map_err(rejected)?;

    let mut discovered = BTreeMap::<NormalizedSourcePath, DiscoveredSource>::new();
    let mut portable_paths = BTreeMap::<String, NormalizedSourcePath>::new();
    portable_paths.insert(entrypoint.portable_identity(), entrypoint.clone());
    let mut pending = BTreeSet::from([entrypoint.clone()]);
    let mut edge_identities = HashSet::<EdgeIdentity>::new();
    let mut resolved_edges = Vec::<ResolvedEdge>::new();
    let mut aggregate_bytes = 0_usize;
    let mut provider_bytes = 0_usize;
    let mut provider_calls = 0_usize;
    let mut rounds = 0_usize;
    let mut import_declarations = 0_usize;
    let mut edge_manifest_bytes = 0_usize;

    while !pending.is_empty() {
        rounds = checked_increment(rounds)?;
        if rounds > MAX_MODULE_DISCOVERY_ROUNDS {
            return Err(budget_rejection("module discovery exceeded the fixed round budget"));
        }
        if discovered.len().checked_add(pending.len()).is_none_or(|count| count > MAX_MODULE_FILES)
        {
            return Err(budget_rejection("module discovery exceeded the fixed file budget"));
        }

        let batch_paths = std::mem::take(&mut pending);
        let mut batch_sources = Vec::with_capacity(batch_paths.len());
        let mut batch_stable = BTreeMap::new();
        let mut batch_bytes = 0_usize;
        for path in batch_paths {
            let stable = source_session.read_source(&path).map_err(rejected)?;
            batch_bytes = checked_add(batch_bytes, stable.text.len())?;
            aggregate_bytes = checked_add(aggregate_bytes, stable.text.len())?;
            if aggregate_bytes > MAX_MODULE_SOURCE_BYTES {
                return Err(budget_rejection(
                    "module discovery exceeded the fixed aggregate source-byte budget",
                ));
            }
            batch_sources.push(SourceFileInput {
                path: path.as_str().to_owned(),
                text: stable.text.clone(),
            });
            batch_stable.insert(path, stable);
        }
        account_provider_bytes(&mut provider_bytes, batch_bytes)?;
        account_provider_call(&mut provider_calls, ProviderCallPhase::Discovery)?;
        let batch_map = SourceMap::build(batch_sources).map_err(|_| invariant_rejection())?;
        source_session.validate_provider_batch(&batch_map).map_err(rejected)?;
        let remaining = remaining_discovery_wall_time(
            discovery_started,
            now(),
            frontend.minimum_analysis_timeout(),
        )?;
        let snapshot = frontend.analyze(&batch_map, remaining);
        enforce_discovery_wall_time(discovery_started, now())?;
        let snapshot = snapshot?;
        reject_provider_errors(&snapshot)?;
        let batch_imports = snapshot_fingerprints(&snapshot);

        for (path, stable) in batch_stable {
            let imports = batch_imports.get(&path).cloned().ok_or_else(invariant_rejection)?;
            for import in &imports {
                import_declarations = checked_increment(import_declarations)?;
                if import_declarations > MAX_MODULE_IMPORT_DECLARATIONS {
                    return Err(budget_rejection(
                        "module discovery exceeded the aggregate import-declaration budget",
                    ));
                }
                let target = resolve_explicit_zry_import(&path, &import.specifier)
                    .map_err(|_| invalid_specifier(&path))?;
                register_portable_path(&mut portable_paths, &target)?;
                for binding in &import.bindings {
                    account_edge_manifest_bytes(
                        &mut edge_manifest_bytes,
                        path.as_str(),
                        target.as_str(),
                        &import.specifier,
                        &binding.imported,
                        &binding.local,
                    )?;
                    let identity = EdgeIdentity {
                        importer: path.clone(),
                        specifier: import.specifier.clone(),
                        imported: binding.imported.clone(),
                        local: binding.local.clone(),
                    };
                    register_edge(
                        &mut edge_identities,
                        &mut resolved_edges,
                        identity,
                        target.clone(),
                    )?;
                }
                if !discovered.contains_key(&target) && !batch_imports.contains_key(&target) {
                    pending.insert(target);
                }
            }
            discovered.insert(path, DiscoveredSource { stable, imports });
        }
    }

    reject_cycles(&discovered, &resolved_edges)?;
    source_session.revalidate_all().map_err(rejected)?;

    let final_inputs = discovered
        .iter()
        .map(|(path, source)| SourceFileInput {
            path: path.as_str().to_owned(),
            text: source.stable.text.clone(),
        })
        .collect();
    let sources = SourceMap::build(final_inputs).map_err(|_| invariant_rejection())?;
    account_provider_bytes(&mut provider_bytes, aggregate_bytes)?;
    account_provider_call(&mut provider_calls, ProviderCallPhase::Final)?;
    let remaining = remaining_discovery_wall_time(
        discovery_started,
        now(),
        frontend.minimum_analysis_timeout(),
    )?;
    let syntax = frontend.analyze(&sources, remaining);
    enforce_discovery_wall_time(discovery_started, now())?;
    let syntax = syntax?;
    reject_provider_errors(&syntax)?;
    source_session.revalidate_all().map_err(rejected)?;
    if !syntax.is_bound_to(&sources) {
        return Err(invariant_rejection());
    }
    let final_imports = snapshot_fingerprints(&syntax);
    for (path, source) in &discovered {
        if final_imports.get(path) != Some(&source.imports) {
            return Err(rejected(module_diagnostic(
                "ZRYNA-D3102",
                Some(path),
                "final authenticated imports differ from fixed-point discovery",
                "use a deterministic exact-v3 provider and retry from unchanged source bytes",
            )));
        }
    }
    if final_imports.len() != discovered.len() {
        return Err(invariant_rejection());
    }

    let modules = discovered
        .iter()
        .enumerate()
        .map(|(index, (path, source))| {
            let id = u32::try_from(index).map_err(|_| invariant_rejection())?;
            Ok(ModuleRecord { id, path: path.clone(), source_sha256: source.stable.sha256 })
        })
        .collect::<Result<Vec<_>, ModuleClosureError>>()?;
    let edges = final_edges(&syntax)?;
    let final_edge_identities = edges
        .iter()
        .map(|edge| EdgeIdentity {
            importer: edge.importer.clone(),
            specifier: edge.specifier.clone(),
            imported: edge.imported.clone(),
            local: edge.local.clone(),
        })
        .collect::<HashSet<_>>();
    if final_edge_identities != edge_identities {
        return Err(rejected(module_diagnostic(
            "ZRYNA-D3102",
            None,
            "final authenticated edge set differs from fixed-point discovery",
            "use a deterministic exact-v3 provider and retry from unchanged source bytes",
        )));
    }
    let graph_sha256 = graph_identity(&entrypoint, &modules, &edges)?;
    enforce_discovery_wall_time(discovery_started, now())?;
    Ok(VerifiedModuleClosure { entrypoint, sources, syntax, modules, edges, graph_sha256 })
}

fn enforce_discovery_wall_time(
    started: Instant,
    current: Instant,
) -> Result<(), ModuleClosureError> {
    if current
        .checked_duration_since(started)
        .is_none_or(|elapsed| elapsed > MAX_MODULE_DISCOVERY_WALL_TIME)
    {
        return Err(budget_rejection("module discovery exceeded the aggregate wall-clock budget"));
    }
    Ok(())
}

fn remaining_discovery_wall_time(
    started: Instant,
    current: Instant,
    minimum_provider_timeout: Duration,
) -> Result<Duration, ModuleClosureError> {
    let elapsed = current.checked_duration_since(started).ok_or_else(|| {
        budget_rejection("module discovery clock moved before the authenticated start")
    })?;
    let remaining = MAX_MODULE_DISCOVERY_WALL_TIME.checked_sub(elapsed).ok_or_else(|| {
        budget_rejection("module discovery exceeded the aggregate wall-clock budget")
    })?;
    if remaining < minimum_provider_timeout {
        return Err(budget_rejection(
            "module discovery left no safe worker cleanup reserve inside the aggregate deadline",
        ));
    }
    Ok(remaining)
}

fn snapshot_fingerprints(
    snapshot: &syntax_v3::ProjectSyntaxSnapshot,
) -> BTreeMap<NormalizedSourcePath, Vec<ImportFingerprint>> {
    snapshot
        .files()
        .iter()
        .map(|file| {
            let imports = file
                .imports()
                .iter()
                .map(|import| ImportFingerprint {
                    span: import.span().into(),
                    import_span: import.import_span().into(),
                    bindings: import
                        .bindings()
                        .iter()
                        .map(|binding| BindingFingerprint {
                            span: binding.span().into(),
                            imported: binding.imported().text().to_owned(),
                            imported_span: binding.imported().span().into(),
                            local: binding.local().text().to_owned(),
                            local_span: binding.local().span().into(),
                            as_span: binding.as_span().map(Into::into),
                        })
                        .collect(),
                    from_span: import.from_span().into(),
                    specifier: import.specifier().text().to_owned(),
                    specifier_token_span: import.specifier().token_span().into(),
                    specifier_value_span: import.specifier().value_span().into(),
                    semicolon_span: import.semicolon_span().into(),
                })
                .collect();
            (file.path().clone(), imports)
        })
        .collect()
}

fn final_edges(
    snapshot: &syntax_v3::ProjectSyntaxSnapshot,
) -> Result<Vec<ModuleEdge>, ModuleClosureError> {
    let mut edges = Vec::new();
    for file in snapshot.files() {
        for import in file.imports() {
            let target = resolve_explicit_zry_import(file.path(), import.specifier().text())
                .map_err(|_| invalid_specifier(file.path()))?;
            for binding in import.bindings() {
                edges.push(ModuleEdge {
                    importer: file.path().clone(),
                    target: target.clone(),
                    specifier: import.specifier().text().to_owned(),
                    imported: binding.imported().text().to_owned(),
                    local: binding.local().text().to_owned(),
                    declaration_span: import.span(),
                    specifier_span: import.specifier().token_span(),
                    imported_span: binding.imported().span(),
                    local_span: binding.local().span(),
                });
            }
        }
    }
    edges.sort_by(|left, right| edge_key(left).cmp(&edge_key(right)));
    Ok(edges)
}

fn edge_key(edge: &ModuleEdge) -> (&[u8], &[u8], &[u8], &[u8]) {
    (
        edge.importer.as_str().as_bytes(),
        edge.specifier.as_bytes(),
        edge.imported.as_bytes(),
        edge.local.as_bytes(),
    )
}

fn register_portable_path(
    paths: &mut BTreeMap<String, NormalizedSourcePath>,
    path: &NormalizedSourcePath,
) -> Result<(), ModuleClosureError> {
    let identity = path.portable_identity();
    if let Some(existing) = paths.get(&identity) {
        if existing != path {
            return Err(rejected(module_diagnostic(
                "ZRYNA-D3005",
                Some(path),
                format!(
                    "module path collides with '{}' when portable ASCII case is ignored",
                    existing.as_str()
                ),
                "use one exact path spelling for every module",
            )));
        }
    } else {
        paths.insert(identity, path.clone());
    }
    Ok(())
}

#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn has_exact_zry_extension(value: &str) -> bool {
    value.ends_with(".zry")
}

fn reject_provider_errors(
    snapshot: &syntax_v3::ProjectSyntaxSnapshot,
) -> Result<(), ModuleClosureError> {
    if snapshot.diagnostics().iter().any(|diagnostic| diagnostic.severity() == Severity::Error) {
        return Err(rejected(module_diagnostic(
            "ZRYNA-D3101",
            None,
            "exact-v3 provider rejected a module-discovery batch",
            "fix the reported source syntax before module discovery",
        )));
    }
    Ok(())
}

fn reject_cycles(
    discovered: &BTreeMap<NormalizedSourcePath, DiscoveredSource>,
    edges: &[ResolvedEdge],
) -> Result<(), ModuleClosureError> {
    let mut outgoing =
        discovered.keys().cloned().map(|path| (path, BTreeSet::new())).collect::<BTreeMap<_, _>>();
    let mut indegree =
        discovered.keys().cloned().map(|path| (path, 0_usize)).collect::<BTreeMap<_, _>>();
    for edge in edges {
        let targets = outgoing.get_mut(&edge.identity.importer).ok_or_else(invariant_rejection)?;
        if targets.insert(edge.target.clone()) {
            let degree = indegree.get_mut(&edge.target).ok_or_else(invariant_rejection)?;
            *degree = checked_increment(*degree)?;
        }
    }
    let mut ready = indegree
        .iter()
        .filter_map(|(path, degree)| (*degree == 0).then_some(path.clone()))
        .collect::<BTreeSet<_>>();
    let mut processed = 0_usize;
    while let Some(path) = ready.pop_first() {
        processed = checked_increment(processed)?;
        if let Some(targets) = outgoing.get(&path) {
            for target in targets {
                let degree = indegree.get_mut(target).ok_or_else(invariant_rejection)?;
                *degree = degree.checked_sub(1).ok_or_else(invariant_rejection)?;
                if *degree == 0 {
                    ready.insert(target.clone());
                }
            }
        }
    }
    if processed != discovered.len() {
        let path = indegree.iter().find_map(|(path, degree)| (*degree > 0).then_some(path));
        return Err(rejected(module_diagnostic(
            "ZRYNA-D3007",
            path,
            "module import graph contains a cycle",
            "remove self imports and cyclic dependency paths",
        )));
    }
    Ok(())
}

fn graph_identity(
    entrypoint: &NormalizedSourcePath,
    modules: &[ModuleRecord],
    edges: &[ModuleEdge],
) -> Result<[u8; 32], ModuleClosureError> {
    crate::source_graph_identity::hash(
        GRAPH_DOMAIN,
        entrypoint,
        modules,
        edges,
        invariant_rejection,
    )
}

fn checked_increment(value: usize) -> Result<usize, ModuleClosureError> {
    checked_add(value, 1)
}

#[derive(Clone, Copy)]
enum ProviderCallPhase {
    Discovery,
    Final,
}

fn account_provider_call(
    calls: &mut usize,
    phase: ProviderCallPhase,
) -> Result<(), ModuleClosureError> {
    *calls = checked_increment(*calls)?;
    let exhausted = match phase {
        ProviderCallPhase::Discovery => *calls >= MAX_MODULE_PROVIDER_CALLS,
        ProviderCallPhase::Final => *calls > MAX_MODULE_PROVIDER_CALLS,
    };
    if exhausted {
        let message = match phase {
            ProviderCallPhase::Discovery => {
                "module discovery left no provider call for final authentication"
            }
            ProviderCallPhase::Final => "module discovery exceeded the fixed provider-call budget",
        };
        return Err(budget_rejection(message));
    }
    Ok(())
}

fn account_provider_bytes(total: &mut usize, input: usize) -> Result<(), ModuleClosureError> {
    *total = checked_add(*total, input)?;
    if *total > MAX_MODULE_PROVIDER_SOURCE_BYTES {
        return Err(budget_rejection(
            "module discovery exceeded the cumulative provider source-byte budget",
        ));
    }
    Ok(())
}

fn account_edge_manifest_bytes(
    total: &mut usize,
    importer: &str,
    target: &str,
    specifier: &str,
    imported_binding: &str,
    local: &str,
) -> Result<(), ModuleClosureError> {
    let raw = [importer, target, specifier, imported_binding, local]
        .into_iter()
        .try_fold(0_usize, |sum, value| checked_add(sum, value.len()))?;
    // Six bytes per input byte covers JSON's longest `\u00XX` escape. The fixed allowance covers
    // keys, punctuation, indentation, and line endings without materializing edge-owned strings.
    let escaped = raw.checked_mul(6).ok_or_else(|| {
        budget_rejection("module edge manifest accounting exceeded the supported integer range")
    })?;
    let estimated = checked_add(escaped, 128)?;
    account_edge_manifest_budget(total, estimated)
}

pub(crate) fn account_edge_manifest_budget(
    total: &mut usize,
    estimated: usize,
) -> Result<(), ModuleClosureError> {
    *total = checked_add(*total, estimated)?;
    if *total > MAX_MODULE_EDGE_MANIFEST_BYTES {
        return Err(budget_rejection(
            "module discovery exceeded the canonical edge-manifest byte budget",
        ));
    }
    Ok(())
}

fn register_edge(
    identities: &mut HashSet<EdgeIdentity>,
    edges: &mut Vec<ResolvedEdge>,
    identity: EdgeIdentity,
    target: NormalizedSourcePath,
) -> Result<(), ModuleClosureError> {
    if identities.contains(&identity) {
        return Err(rejected(module_diagnostic(
            "ZRYNA-D3006",
            Some(&identity.importer),
            "duplicate named-import edge is not permitted",
            "remove the repeated imported/local binding edge",
        )));
    }
    if identities.len() >= MAX_MODULE_IMPORT_EDGES {
        return Err(budget_rejection(
            "module discovery exceeded the fixed named-import edge budget",
        ));
    }
    identities.insert(identity.clone());
    edges.push(ResolvedEdge { identity, target });
    Ok(())
}

fn checked_add(left: usize, right: usize) -> Result<usize, ModuleClosureError> {
    left.checked_add(right).ok_or_else(|| {
        budget_rejection("module discovery accounting exceeded the supported integer range")
    })
}

fn invalid_specifier(importer: &NormalizedSourcePath) -> ModuleClosureError {
    rejected(module_diagnostic(
        "ZRYNA-D3001",
        Some(importer),
        "module specifier does not resolve to one explicit portable relative .zry path",
        "use an explicit ./ or ../ named .zry import that remains inside the workspace root",
    ))
}

fn budget_rejection(message: &'static str) -> ModuleClosureError {
    rejected(module_diagnostic(
        "ZRYNA-D3201",
        None,
        message,
        "reduce the module graph before deterministic discovery",
    ))
}

fn invariant_rejection() -> ModuleClosureError {
    rejected(module_diagnostic(
        "ZRYNA-D3102",
        None,
        "module closure violated an internal source-map binding invariant",
        "report this compiler invariant failure with the smallest reproducible workspace",
    ))
}

fn rejected(diagnostic: Diagnostic) -> ModuleClosureError {
    ModuleClosureError::Rejected(vec![diagnostic])
}

fn module_diagnostic(
    code: &'static str,
    path: Option<&NormalizedSourcePath>,
    message: impl Into<String>,
    guidance: &'static str,
) -> Diagnostic {
    let message = message.into();
    let message = path.map_or(message.clone(), |path| format!("{}: {message}", path.as_str()));
    Diagnostic::error(code, None, message, guidance)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use zryna_source::{NormalizedSourcePath, resolve_explicit_zry_import};

    use super::{
        EdgeIdentity, MAX_MODULE_IMPORT_EDGES, MAX_MODULE_PROVIDER_CALLS,
        MAX_MODULE_PROVIDER_SOURCE_BYTES, ProviderCallPhase, ResolvedEdge, account_provider_bytes,
        account_provider_call, register_edge,
    };

    fn path(value: &str) -> NormalizedSourcePath {
        NormalizedSourcePath::new(value).expect("test path must be normalized")
    }

    #[test]
    fn resolver_accepts_only_explicit_normalized_in_root_zry_paths() {
        let importer = path("src/nested/main.zry");
        for (specifier, expected) in [
            ("./dep.zry", "src/nested/dep.zry"),
            ("./child/../dep.zry", "src/nested/dep.zry"),
            ("../shared.zry", "src/shared.zry"),
            ("../../root.zry", "root.zry"),
        ] {
            assert_eq!(
                resolve_explicit_zry_import(&importer, specifier)
                    .expect("specifier must resolve")
                    .as_str(),
                expected
            );
        }

        for rejected in [
            "",
            "dep.zry",
            "/dep.zry",
            "C:/dep.zry",
            "//server/dep.zry",
            "https://example.invalid/dep.zry",
            "./dep",
            "./dep.ZRY",
            "./dep.zry?query",
            "./dep.zry#fragment",
            ".\\dep.zry",
            "../../../escape.zry",
        ] {
            assert!(
                resolve_explicit_zry_import(&importer, rejected).is_err(),
                "specifier must be rejected: {rejected}"
            );
        }
    }

    #[test]
    fn edge_registry_accepts_exact_limit_and_rejects_duplicate_or_first_extra_edge() {
        let importer = path("main.zry");
        let target = path("dep.zry");
        let mut identities = HashSet::new();
        let mut edges = Vec::<ResolvedEdge>::new();
        for index in 0..MAX_MODULE_IMPORT_EDGES {
            register_edge(
                &mut identities,
                &mut edges,
                EdgeIdentity {
                    importer: importer.clone(),
                    specifier: "./dep.zry".to_owned(),
                    imported: "value".to_owned(),
                    local: format!("local{index}"),
                },
                target.clone(),
            )
            .expect("every edge through the exact limit must succeed");
        }
        assert_eq!(identities.len(), MAX_MODULE_IMPORT_EDGES);
        assert_eq!(edges.len(), MAX_MODULE_IMPORT_EDGES);

        let duplicate = register_edge(
            &mut identities,
            &mut edges,
            EdgeIdentity {
                importer: importer.clone(),
                specifier: "./dep.zry".to_owned(),
                imported: "value".to_owned(),
                local: "local0".to_owned(),
            },
            target.clone(),
        )
        .expect_err("a duplicate remains a duplicate at the exact limit");
        assert_eq!(duplicate.diagnostics()[0].code(), "ZRYNA-D3006");

        let extra = register_edge(
            &mut identities,
            &mut edges,
            EdgeIdentity {
                importer,
                specifier: "./dep.zry".to_owned(),
                imported: "value".to_owned(),
                local: "firstExtra".to_owned(),
            },
            target,
        )
        .expect_err("the first extra edge must fail");
        assert_eq!(extra.diagnostics()[0].code(), "ZRYNA-D3201");
    }

    #[test]
    fn provider_accounting_accepts_exact_call_and_byte_limits_and_rejects_first_extra() {
        let mut calls = 0;
        for _ in 0..MAX_MODULE_PROVIDER_CALLS - 1 {
            account_provider_call(&mut calls, ProviderCallPhase::Discovery)
                .expect("every discovery call that reserves the final call must succeed");
        }
        account_provider_call(&mut calls, ProviderCallPhase::Final)
            .expect("the exact final provider call must succeed");
        assert_eq!(calls, MAX_MODULE_PROVIDER_CALLS);
        let extra_call = account_provider_call(&mut calls, ProviderCallPhase::Final)
            .expect_err("the first extra provider call must fail");
        assert_eq!(extra_call.diagnostics()[0].code(), "ZRYNA-D3201");

        let mut no_final_call = MAX_MODULE_PROVIDER_CALLS - 1;
        let reserved_call = account_provider_call(&mut no_final_call, ProviderCallPhase::Discovery)
            .expect_err("discovery must reserve exactly one final provider call");
        assert_eq!(reserved_call.diagnostics()[0].code(), "ZRYNA-D3201");

        let mut bytes = 0;
        account_provider_bytes(&mut bytes, MAX_MODULE_PROVIDER_SOURCE_BYTES)
            .expect("the exact cumulative provider-byte limit must succeed");
        assert_eq!(bytes, MAX_MODULE_PROVIDER_SOURCE_BYTES);
        let extra_byte = account_provider_bytes(&mut bytes, 1)
            .expect_err("the first extra cumulative provider byte must fail");
        assert_eq!(extra_byte.diagnostics()[0].code(), "ZRYNA-D3201");
    }
}
