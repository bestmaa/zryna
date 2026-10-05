//! Private source-to-object selection; no linking, host grant or public command route.

use zryna_backend_native::native_c_v0::resources::ValidatedHandleEntries;
use zryna_diagnostics::Diagnostic;
use zryna_semantics::native_c_v0::{
    LibraryMaterial,
    body::{compose_private_boundaries, verify_bodies},
};
use zryna_source::{SourceMap, Span};

/// Admits the original immutable source/materials before dispatching to the native emitter.
/// The declaration verifier owns unsupported-target rejection; no fallback backend is selected.
pub(super) fn emit_handle_entry(
    sources: &SourceMap,
    declaration_bytes: &[u8],
    materials: &[LibraryMaterial<'_>],
    selected_target: &str,
    function_name: &str,
) -> Result<ValidatedHandleEntries, Vec<Diagnostic>> {
    let syntax = zryna_syntax::native_c_source_v0::authenticate_sources(sources)
        .map_err(|error| rejection(error.code(), error.detail(), None))?;
    let declarations = zryna_semantics::native_c_v0::verify_report(
        declaration_bytes,
        sources,
        &syntax,
        materials,
        selected_target,
    )
    .map_err(|errors| {
        errors
            .into_iter()
            .flat_map(|error| rejection(error.code(), error.detail(), None))
            .collect::<Vec<_>>()
    })?;
    let bodies = verify_bodies(sources, &declarations)
        .map_err(|error| rejection(error.code(), error.detail(), error.span()))?;
    let private = compose_private_boundaries(sources, &bodies).map_err(|error| {
        if error.layout_diagnostics().is_empty() {
            rejection(error.code(), error.detail(), error.span())
        } else {
            error.layout_diagnostics().to_vec()
        }
    })?;
    let ir =
        zryna_native_c_ir::lower(sources, &private).map_err(|error| vec![error.diagnostic()])?;
    let mir =
        zryna_native_mir::native_c_v0::lower(&ir).map_err(|error| vec![error.diagnostic()])?;
    let symbol = mir
        .functions()
        .find(|function| function.name() == function_name)
        .ok_or_else(|| rejection("ZRYNA-C4104", "native-C source entry selection", None))?
        .entry()
        .symbol
        .clone();
    let target =
        zryna_backend_native::select_object_target(selected_target).map_err(|error| vec![error])?;
    // Observe the actual dispatch edge, not a caller-supplied substitute emitter.
    #[cfg(test)]
    EMITTER_ENTRIES.with(|entries| entries.set(entries.get() + 1));
    zryna_backend_native::native_c_v0::resources::emit_handle_entries(&mir, &[&symbol], target)
        .map_err(|error| vec![error])
}

fn rejection(code: &str, detail: &str, span: Option<Span>) -> Vec<Diagnostic> {
    let guidance = "supply original authenticated native-C source and exact captured materials";
    vec![span.map_or_else(
        || Diagnostic::error(code, None, detail, guidance),
        |span| Diagnostic::error_at(code, span, detail, guidance),
    )]
}

#[cfg(test)]
std::thread_local! {
    static EMITTER_ENTRIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests;
