//! Source-derived graph authentication independently of the full syntax producer.

use std::collections::HashSet;

use sha2::{Digest as _, Sha256};
use zryna_diagnostics::Diagnostic;
use zryna_frontend::{native_lexer, native_parser};
use zryna_source::{SourceMap, resolve_explicit_zry_import};

use super::{ModuleClosureError, NativeSourceSnapshot};
use crate::module_closure::{
    MAX_MODULE_IMPORT_DECLARATIONS, MAX_MODULE_IMPORT_EDGES, ModuleEdge,
    account_edge_manifest_bytes, budget_rejection, checked_add, edge_key, graph_identity,
    invariant_rejection,
};

pub(super) fn failure(code: &'static str, message: &'static str) -> ModuleClosureError {
    ModuleClosureError::Rejected(vec![Diagnostic::error(
        code,
        None,
        message,
        "use one unchanged exact admitted source closure",
    )])
}

pub(super) fn discover(
    sources: &SourceMap,
) -> Result<Vec<native_parser::v3::RawModuleImports>, ModuleClosureError> {
    let lexed = native_lexer::lex(sources)
        .map_err(|error| ModuleClosureError::Rejected(vec![error.diagnostic().clone()]))?;
    native_parser::v3::discover_import_candidates(sources, &lexed)
        .map_err(|error| ModuleClosureError::Rejected(vec![error.diagnostic().clone()]))
}

pub(super) fn candidate_edges(sources: &SourceMap) -> Result<Vec<ModuleEdge>, ModuleClosureError> {
    let mut edges = Vec::new();
    let mut identities = HashSet::new();
    let mut imports_seen = 0;
    let mut manifest_bytes = 0;
    for file in discover(sources)? {
        let importer = sources
            .verify_file_id(file.id)
            .ok()
            .and_then(|id| sources.source(id))
            .ok_or_else(invariant_rejection)?
            .path();
        for import in file.imports {
            imports_seen = checked_add(imports_seen, 1)?;
            if imports_seen > MAX_MODULE_IMPORT_DECLARATIONS {
                return Err(budget_rejection("native source graph exceeded its import budget"));
            }
            let target = resolve_explicit_zry_import(importer, &import.specifier.text)
                .map_err(|_| crate::module_closure::invalid_specifier(importer))?;
            if sources.file_id(&target).is_none() {
                return Err(failure(
                    "ZRYNA-D3102",
                    "native graph contains an absent source target",
                ));
            }
            for binding in import.bindings {
                let identity = (
                    importer.clone(),
                    import.specifier.text.clone(),
                    binding.imported.text.clone(),
                    binding.local.text.clone(),
                );
                if !identities.insert(identity) {
                    return Err(failure("ZRYNA-D3006", "duplicate named-import edge"));
                }
                if edges.len() >= MAX_MODULE_IMPORT_EDGES {
                    return Err(budget_rejection("native source graph exceeded its edge budget"));
                }
                account_edge_manifest_bytes(
                    &mut manifest_bytes,
                    importer.as_str(),
                    target.as_str(),
                    &import.specifier.text,
                    &binding.imported.text,
                    &binding.local.text,
                )?;
                let span = |raw| sources.verify_span(raw).map_err(|_| invariant_rejection());
                edges.push(ModuleEdge {
                    importer: importer.clone(),
                    target: target.clone(),
                    specifier: import.specifier.text.clone(),
                    imported: binding.imported.text,
                    local: binding.local.text,
                    declaration_span: span(import.span)?,
                    specifier_span: span(import.specifier.token_span)?,
                    imported_span: span(binding.imported.span)?,
                    local_span: span(binding.local.span)?,
                });
            }
        }
    }
    edges.sort_by(|left, right| edge_key(left).cmp(&edge_key(right)));
    Ok(edges)
}

pub(super) fn authenticate(snapshot: &NativeSourceSnapshot<'_>) -> Result<(), ModuleClosureError> {
    if snapshot.sources.identity() != snapshot.source_identity
        || snapshot.sources.len() != snapshot.modules.len()
        || snapshot.sources.file_id(&snapshot.entrypoint).is_none()
    {
        return Err(failure("ZRYNA-D3102", "native source file set differs from its sealed graph"));
    }
    for (index, module) in snapshot.modules.iter().enumerate() {
        let id = snapshot.sources.verify_file_id(module.id).map_err(|_| invariant_rejection())?;
        let source = snapshot.sources.source(id).ok_or_else(invariant_rejection)?;
        let sha256: [u8; 32] = Sha256::digest(source.text().as_bytes()).into();
        if usize::try_from(module.id).ok() != Some(index)
            || source.path() != &module.path
            || sha256 != module.source_sha256
        {
            return Err(failure(
                "ZRYNA-D3102",
                "native source path or hash differs from its sealed graph",
            ));
        }
    }
    let edges = candidate_edges(&snapshot.sources)?;
    if edges != snapshot.edges {
        return Err(failure(
            "ZRYNA-D3102",
            "native import edges differ from their original source",
        ));
    }
    crate::ownership_closure::validate_native_graph(&snapshot.modules, &edges)?;
    if graph_identity(&snapshot.entrypoint, &snapshot.modules, &edges)? != snapshot.graph_v3
        || crate::ownership_closure::native_graph_identity(
            &snapshot.entrypoint,
            &snapshot.modules,
            &edges,
        )? != snapshot.graph_v4
    {
        return Err(failure(
            "ZRYNA-D3102",
            "native graph identity differs from its sealed sources",
        ));
    }
    Ok(())
}
