use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};
use zryna_diagnostics::{Diagnostic, Severity};
use zryna_frontend::syntax_v4;
use zryna_source::{NormalizedSourcePath, SourceMap, Span, UntrustedSpan};

use super::{
    DiscoveredSource, GRAPH_DOMAIN, GRAPH_VERSION, Import, ImportBinding,
    MAX_MODULE_PROVIDER_CALLS, MAX_MODULE_PROVIDER_SOURCE_BYTES, ModuleClosureError, ModuleEdge,
    ModuleRecord, add,
};

pub(super) fn imports(
    snapshot: &syntax_v4::ProjectSyntaxSnapshot,
) -> BTreeMap<NormalizedSourcePath, Vec<Import>> {
    snapshot
        .files()
        .iter()
        .map(|file| {
            let imports = file
                .imports()
                .iter()
                .map(|import| Import {
                    span: import.span,
                    specifier: import.specifier.text.clone(),
                    specifier_span: import.specifier.token_span,
                    bindings: import
                        .bindings
                        .iter()
                        .map(|binding| ImportBinding {
                            imported: binding.imported.text.clone(),
                            local: binding.local.text.clone(),
                            imported_span: binding.imported.span,
                            local_span: binding.local.span,
                        })
                        .collect(),
                })
                .collect();
            (file.path().clone(), imports)
        })
        .collect()
}

pub(super) fn imports_match(
    final_imports: &BTreeMap<NormalizedSourcePath, Vec<Import>>,
    discovered: &BTreeMap<NormalizedSourcePath, DiscoveredSource>,
) -> bool {
    final_imports.len() == discovered.len()
        && discovered.iter().all(|(path, source)| {
            final_imports
                .get(path)
                .is_some_and(|imports| import_lists_match(imports, &source.imports))
        })
}

fn import_lists_match(left: &[Import], right: &[Import]) -> bool {
    // Discovery batches and the final closure have distinct source maps. The normalized path key
    // identifies the file here, so compare source offsets without comparing map-local file IDs.
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            span_offsets_match(left.span, right.span)
                && left.specifier == right.specifier
                && span_offsets_match(left.specifier_span, right.specifier_span)
                && left.bindings.len() == right.bindings.len()
                && left.bindings.iter().zip(&right.bindings).all(|(left, right)| {
                    left.imported == right.imported
                        && left.local == right.local
                        && span_offsets_match(left.imported_span, right.imported_span)
                        && span_offsets_match(left.local_span, right.local_span)
                })
        })
}

const fn span_offsets_match(left: UntrustedSpan, right: UntrustedSpan) -> bool {
    left.start == right.start && left.end == right.end
}

pub(super) fn final_edges(
    syntax: &syntax_v4::ProjectSyntaxSnapshot,
    sources: &SourceMap,
) -> Result<Vec<ModuleEdge>, ModuleClosureError> {
    let mut edges = Vec::new();
    for (path, imports) in imports(syntax) {
        for import in imports {
            let target = zryna_source::resolve_explicit_zry_import(&path, &import.specifier)
                .map_err(|_| invalid_import(&path))?;
            for binding in import.bindings {
                edges.push(ModuleEdge {
                    importer: path.clone(),
                    target: target.clone(),
                    specifier: import.specifier.clone(),
                    imported: binding.imported,
                    local: binding.local,
                    declaration_span: verified_span(sources, import.span)?,
                    specifier_span: verified_span(sources, import.specifier_span)?,
                    imported_span: verified_span(sources, binding.imported_span)?,
                    local_span: verified_span(sources, binding.local_span)?,
                });
            }
        }
    }
    edges.sort_by(|a, b| {
        (&a.importer, &a.specifier, &a.imported, &a.local).cmp(&(
            &b.importer,
            &b.specifier,
            &b.imported,
            &b.local,
        ))
    });
    Ok(edges)
}

fn verified_span(sources: &SourceMap, span: UntrustedSpan) -> Result<Span, ModuleClosureError> {
    sources.verify_span(span).map_err(|_| invariant())
}

pub(super) fn reject_provider_errors(
    snapshot: &syntax_v4::ProjectSyntaxSnapshot,
) -> Result<(), ModuleClosureError> {
    if snapshot.diagnostics().iter().any(|item| item.severity() == Severity::Error) {
        return Err(rejected(diagnostic(
            "ZRYNA-D3301",
            None,
            "exact-v4 provider rejected an ownership module-discovery batch",
        )));
    }
    Ok(())
}

pub(super) fn reject_cycles(
    modules: &[ModuleRecord],
    edges: &[ModuleEdge],
) -> Result<(), ModuleClosureError> {
    let mut outgoing = modules
        .iter()
        .map(|module| (module.path.clone(), BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    let mut indegree =
        modules.iter().map(|module| (module.path.clone(), 0_usize)).collect::<BTreeMap<_, _>>();
    for edge in edges {
        if outgoing.get_mut(&edge.importer).ok_or_else(invariant)?.insert(edge.target.clone()) {
            let count = indegree.get_mut(&edge.target).ok_or_else(invariant)?;
            *count = add(*count, 1)?;
        }
    }
    let mut ready = indegree
        .iter()
        .filter_map(|(path, count)| (*count == 0).then_some(path.clone()))
        .collect::<BTreeSet<_>>();
    let mut visited = 0_usize;
    while let Some(path) = ready.pop_first() {
        visited = add(visited, 1)?;
        for target in outgoing.get(&path).ok_or_else(invariant)? {
            let count = indegree.get_mut(target).ok_or_else(invariant)?;
            *count = count.checked_sub(1).ok_or_else(invariant)?;
            if *count == 0 {
                ready.insert(target.clone());
            }
        }
    }
    if visited != modules.len() {
        return Err(rejected(diagnostic(
            "ZRYNA-D3301",
            None,
            "ownership module import graph contains a cycle",
        )));
    }
    Ok(())
}

pub(super) fn register_portable(
    paths: &mut BTreeMap<String, NormalizedSourcePath>,
    path: &NormalizedSourcePath,
) -> Result<(), ModuleClosureError> {
    let identity = path.portable_identity();
    if paths.get(&identity).is_some_and(|existing| existing != path) {
        return Err(rejected(diagnostic(
            "ZRYNA-D3301",
            Some(path),
            "ownership module path has a portable case collision",
        )));
    }
    paths.insert(identity, path.clone());
    Ok(())
}

pub(super) fn graph_identity(
    entrypoint: &NormalizedSourcePath,
    modules: &[ModuleRecord],
    edges: &[ModuleEdge],
) -> Result<[u8; 32], ModuleClosureError> {
    let mut bytes = GRAPH_DOMAIN.to_vec();
    push_u32(&mut bytes, GRAPH_VERSION)?;
    push_text(&mut bytes, entrypoint.as_str())?;
    push_u32(&mut bytes, modules.len())?;
    for module in modules {
        push_text(&mut bytes, module.path.as_str())?;
        bytes.extend_from_slice(&module.source_sha256);
    }
    push_u32(&mut bytes, edges.len())?;
    for edge in edges {
        for value in [edge.importer.as_str(), &edge.specifier, &edge.imported, &edge.local] {
            push_text(&mut bytes, value)?;
        }
    }
    Ok(Sha256::digest(bytes).into())
}

fn push_text(bytes: &mut Vec<u8>, value: &str) -> Result<(), ModuleClosureError> {
    push_u32(bytes, value.len())?;
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

fn push_u32(bytes: &mut Vec<u8>, value: impl TryInto<u32>) -> Result<(), ModuleClosureError> {
    bytes.extend_from_slice(&value.try_into().map_err(|_| invariant())?.to_le_bytes());
    Ok(())
}

pub(super) fn account_provider(
    calls: &mut usize,
    bytes: &mut usize,
    input_bytes: usize,
    final_call: bool,
) -> Result<(), ModuleClosureError> {
    *calls = add(*calls, 1)?;
    *bytes = add(*bytes, input_bytes)?;
    if *bytes > MAX_MODULE_PROVIDER_SOURCE_BYTES
        || *calls > MAX_MODULE_PROVIDER_CALLS
        || (!final_call && *calls == MAX_MODULE_PROVIDER_CALLS)
    {
        return Err(budget("ownership module discovery exceeded its provider budget"));
    }
    Ok(())
}

pub(super) fn remaining_with_limit(
    started: Instant,
    now: Instant,
    minimum: Duration,
    limit: Duration,
) -> Result<Duration, ModuleClosureError> {
    let elapsed = now.checked_duration_since(started).ok_or_else(invariant)?;
    let remaining = limit
        .checked_sub(elapsed)
        .ok_or_else(|| budget("ownership module discovery exceeded its wall-clock budget"))?;
    if remaining < minimum {
        return Err(budget("ownership module discovery exhausted its worker cleanup reserve"));
    }
    Ok(remaining)
}

pub(super) fn enforce_time_limit(
    started: Instant,
    now: Instant,
    limit: Duration,
) -> Result<(), ModuleClosureError> {
    remaining_with_limit(started, now, Duration::ZERO, limit).map(|_| ())
}

pub(super) fn invalid_import(path: &NormalizedSourcePath) -> ModuleClosureError {
    rejected(diagnostic(
        "ZRYNA-D3301",
        Some(path),
        "ownership import does not resolve to an explicit in-root relative .zry path",
    ))
}

pub(super) fn invariant() -> ModuleClosureError {
    rejected(diagnostic(
        "ZRYNA-D3302",
        None,
        "ownership closure violated an internal source-map authority invariant",
    ))
}

pub(super) fn budget(message: &'static str) -> ModuleClosureError {
    rejected(diagnostic("ZRYNA-D3303", None, message))
}

pub(super) fn rejected(item: Diagnostic) -> ModuleClosureError {
    ModuleClosureError::Rejected(vec![item])
}

pub(super) fn diagnostic(
    code: &'static str,
    path: Option<&NormalizedSourcePath>,
    message: impl Into<String>,
) -> Diagnostic {
    let message = message.into();
    Diagnostic::error(
        code,
        None,
        path.map_or(message.clone(), |path| format!("{}: {message}", path.as_str())),
        "use one unchanged bounded exact-v4 ownership source closure",
    )
}
