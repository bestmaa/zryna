//! Bounded native extension: genuine pure issuers plus a separate authenticated C issuer.
//! This private seal grants no execution, linking, publication or WIT capability.

use std::collections::{BTreeMap, BTreeSet};

use zryna_backend_native::native_c_v0::resources::ValidatedHandleEntries;
use zryna_diagnostics::Diagnostic;
use zryna_semantics::native_c_v0::VerifiedDeclarationSet;
use zryna_source::{SourceMap, SourceMapIdentity};

use super::{
    INVALID, PROFILE, UNSUPPORTED,
    authority::{Authorities, Binding},
    error,
    graph::{self, Graph},
    model::{Input, Reservation, Row},
    policy::Policy,
};

#[derive(Clone, Debug)]
struct NativeAuthority {
    sources: SourceMap,
    declarations: VerifiedDeclarationSet,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeBinding {
    declaration_sha256: [u8; 32],
    source_identity: SourceMapIdentity,
}

impl NativeAuthority {
    fn binding(&self) -> Result<NativeBinding, Vec<Diagnostic>> {
        if !self.declarations.belongs_to(&self.sources) || self.declarations.operation_count() == 0
        {
            return Err(vec![error(
                INVALID,
                "native requirement lacks its original C source issuer",
            )]);
        }
        Ok(NativeBinding {
            declaration_sha256: *self.declarations.declaration_sha256(),
            source_identity: self.sources.identity(),
        })
    }
}

type NativeAuthorities = BTreeMap<String, NativeAuthority>;
type SourceIdentities = BTreeMap<String, Vec<SourceMapIdentity>>;
type Closures = BTreeMap<String, BTreeSet<String>>;
type Witnesses = BTreeMap<String, Vec<String>>;

#[derive(Debug)]
struct ValidatedNativeComposition {
    input: Input,
    graph_binding: [u8; 32],
    pure: Binding,
    pure_sources: SourceIdentities,
    native: BTreeMap<String, NativeBinding>,
    closures: Closures,
    witnesses: Witnesses,
    issuers: NativeAuthorities,
}

impl ValidatedNativeComposition {
    fn revalidate(
        &self,
        input: &Input,
        pure: &Authorities,
        native: &NativeAuthorities,
    ) -> Result<(), Vec<Diagnostic>> {
        let checked = verify(input, pure, native)?;
        if self.input != checked.input
            || self.graph_binding != checked.graph_binding
            || self.pure != checked.pure
            || self.pure_sources != checked.pure_sources
            || self.native != checked.native
            || self.closures != checked.closures
            || self.witnesses != checked.witnesses
        {
            return Err(vec![error(
                INVALID,
                "native composition graph, issuer or derived witness changed",
            )]);
        }
        // The retained emitter issuer is independently compared, not replaced by the caller.
        for (id, issuer) in &self.issuers {
            if self.native.get(id) != Some(&issuer.binding()?) {
                return Err(vec![error(INVALID, "retained native emitter issuer changed")]);
            }
        }
        if self.issuers.keys().ne(self.native.keys()) {
            return Err(vec![error(INVALID, "retained native emitter issuer map changed")]);
        }
        Ok(())
    }

    fn emit_entry(
        &self,
        input: &Input,
        pure: &Authorities,
        native: &NativeAuthorities,
        issuer: &str,
        function: &str,
    ) -> Result<ValidatedHandleEntries, Vec<Diagnostic>> {
        self.revalidate(input, pure, native)?;
        let authority = self
            .issuers
            .get(issuer)
            .ok_or_else(|| vec![error(INVALID, "selected native issuer is absent")])?;
        crate::native::native_c_v0::source_selection::emit_authenticated_handle_entry(
            &authority.sources,
            &authority.declarations,
            zryna_syntax::native_c_v0::TARGET,
            function,
        )
    }
}

fn verify(
    input: &Input,
    pure: &Authorities,
    native: &NativeAuthorities,
) -> Result<ValidatedNativeComposition, Vec<Diagnostic>> {
    let graph = graph::validate(input)?;
    let pure_ids: BTreeSet<_> = pure.instances.keys().cloned().collect();
    let native_ids: BTreeSet<_> = native.keys().cloned().collect();
    if native_ids.is_empty()
        || !pure_ids.is_disjoint(&native_ids)
        || pure_ids.union(&native_ids).cloned().collect::<BTreeSet<_>>() != graph.ids()
    {
        return Err(vec![error(
            INVALID,
            "pure and native issuers must partition the fixed graph exactly",
        )]);
    }
    let pure_binding = pure.binding(&pure_ids)?;
    if pure_ids.iter().any(|id| !pure_binding.has_language(id, graph.input.language)) {
        return Err(vec![error(PROFILE, "pure instance lacks the selected sealed language")]);
    }
    pure_binding.validate_command_requirements(&graph.input)?;
    let native_binding = native
        .iter()
        .map(|(id, issuer)| issuer.binding().map(|binding| (id.clone(), binding)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let (closures, witnesses) = derive(&graph, &native_ids);
    select(&graph, pure, &witnesses)?;
    Ok(ValidatedNativeComposition {
        graph_binding: graph.binding(&pure_binding)?,
        input: graph.input,
        pure: pure_binding,
        pure_sources: pure.source_identities()?,
        native: native_binding,
        closures,
        witnesses,
        issuers: native.clone(),
    })
}

fn derive(graph: &Graph, native: &BTreeSet<String>) -> (Closures, Witnesses) {
    let root_paths = graph.paths(&graph.input.root);
    let witnesses = native.iter().map(|id| (id.clone(), root_paths[id].clone())).collect();
    let closures = graph
        .input
        .instances
        .iter()
        .map(|node| {
            let paths = graph.paths(&node.id);
            let required = native.iter().filter(|id| paths.contains_key(*id)).cloned().collect();
            (node.id.clone(), required)
        })
        .collect();
    (closures, witnesses)
}

fn select(graph: &Graph, pure: &Authorities, witnesses: &Witnesses) -> Result<(), Vec<Diagnostic>> {
    // Report the actual root-to-C requirement before considering any selected emitter.
    if graph.input.selections.iter().any(|selection| selection.row != Row::NativeHost) {
        let path = witnesses
            .values()
            .min_by_key(|path| (path.len(), *path))
            .ok_or_else(|| vec![error(INVALID, "native witness is absent")])?;
        return Err(vec![error(
            "ZRYNA-C4103",
            format!(
                "native-only foreign requirement; root {}; witness {}",
                graph.input.root,
                path.join(" -> "),
            ),
        )]);
    }
    if graph.input.selections.len() != 1 {
        return Err(vec![error(INVALID, "duplicate selected native output")]);
    }
    let policy = Policy::load()?;
    policy.selection(&graph.input.selections[0]).map_err(|error| vec![error])?;
    // This increment admits no WIT/effect requests or quotas. Reject broader claims explicitly;
    // no existing capability restriction, resource gate or public composition route is relaxed.
    if pure.wit.is_some()
        || !graph.input.selections[0].approved.is_empty()
        || graph.input.instances.iter().any(|node| {
            !node.rows.contains(&Row::NativeHost)
                || !node.requirements.is_empty()
                || node.reservation != Reservation::default()
        })
    {
        return Err(vec![error(
            UNSUPPORTED,
            "private native composition requires compatible rows and no host effects or quotas",
        )]);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
