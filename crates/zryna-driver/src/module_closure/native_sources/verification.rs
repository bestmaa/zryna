//! Mandatory versioned syntax and closure verification over one sealed source authority.

use zryna_frontend::{native_lexer, native_parser, syntax_v2, syntax_v3};
use zryna_source::SourceMap;

use super::{ModuleClosureError, NativeSourceSnapshot, VerifiedModuleClosure, final_edges, graph};
use crate::VerifiedOwnershipModuleClosure;

/// Native v2 syntax and its retained immutable source owner.
pub struct NativeSyntaxSnapshot<'root> {
    source: NativeSourceSnapshot<'root>,
    syntax: syntax_v2::ProjectSyntaxSnapshot,
}

impl NativeSyntaxSnapshot<'_> {
    /// Returns the original immutable source map.
    #[must_use]
    pub const fn sources(&self) -> &SourceMap {
        &self.source.sources
    }
    /// Returns mandatory-verifier-authenticated v2 syntax.
    #[must_use]
    pub const fn syntax(&self) -> &syntax_v2::ProjectSyntaxSnapshot {
        &self.syntax
    }
    /// Revalidates retained handles and immutable graph/hash bindings.
    /// # Errors
    /// Rejects a stale source owner or inconsistent graph.
    pub fn revalidate(&self) -> Result<(), ModuleClosureError> {
        self.source.revalidate()
    }
}

/// Native v3 closure retained with the source capabilities that issued it.
pub struct NativeModuleSnapshot<'root> {
    source: NativeSourceSnapshot<'root>,
    closure: VerifiedModuleClosure,
}

impl NativeModuleSnapshot<'_> {
    /// Returns the existing authenticated driver closure over the original source map.
    #[must_use]
    pub const fn closure(&self) -> &VerifiedModuleClosure {
        &self.closure
    }
    /// Revalidates retained handles and immutable graph/hash bindings before dispatch.
    /// # Errors
    /// Rejects stale source authority.
    pub fn revalidate(&self) -> Result<(), ModuleClosureError> {
        self.source.revalidate()
    }
}

/// Native v4 closure retained with its workspace or exact admitted package source owner.
pub struct NativeOwnershipSnapshot<'root> {
    source: NativeSourceSnapshot<'root>,
    closure: VerifiedOwnershipModuleClosure,
}

impl NativeOwnershipSnapshot<'_> {
    /// Returns mandatory-verifier-authenticated ownership syntax and graph.
    #[must_use]
    pub const fn closure(&self) -> &VerifiedOwnershipModuleClosure {
        &self.closure
    }
    /// Revalidates retained handles and immutable graph/hash bindings before dispatch.
    /// # Errors
    /// Rejects stale source authority.
    pub fn revalidate(&self) -> Result<(), ModuleClosureError> {
        self.source.revalidate()
    }
}

impl<'root> NativeSourceSnapshot<'root> {
    /// Authenticates complete native M1 syntax against the original source map.
    /// # Errors
    /// Rejects stale capabilities, unsupported syntax and malformed versioned candidates.
    pub fn verify_v2(self) -> Result<NativeSyntaxSnapshot<'root>, ModuleClosureError> {
        self.revalidate()?;
        let lexed = native_lexer::lex(&self.sources)
            .map_err(|error| ModuleClosureError::Rejected(vec![error.diagnostic().clone()]))?;
        let raw = native_parser::parse_v2_recovering_candidate(&self.sources, &lexed)
            .map_err(|error| ModuleClosureError::Rejected(vec![error.diagnostic().clone()]))?;
        let syntax =
            syntax_v2::verify_snapshot(raw, &self.sources).map_err(ModuleClosureError::Rejected)?;
        self.revalidate()?;
        Ok(NativeSyntaxSnapshot { source: self, syntax })
    }

    /// Authenticates complete native M2 syntax and the presealed canonical driver graph.
    /// # Errors
    /// Rejects source substitution, parser defects, omitted imports and versioned syntax failure.
    pub fn verify_v3(self) -> Result<NativeModuleSnapshot<'root>, ModuleClosureError> {
        self.revalidate()?;
        let lexed = native_lexer::lex(&self.sources)
            .map_err(|error| ModuleClosureError::Rejected(vec![error.diagnostic().clone()]))?;
        let raw = native_parser::v3::parse_v3_straight_line_candidate(&self.sources, &lexed)
            .map_err(|error| ModuleClosureError::Rejected(vec![error.diagnostic().clone()]))?;
        let syntax =
            syntax_v3::verify_snapshot(raw, &self.sources).map_err(ModuleClosureError::Rejected)?;
        crate::module_closure::reject_provider_errors(&syntax)?;
        if !syntax.is_bound_to(&self.sources) || final_edges(&syntax)? != self.edges {
            return Err(graph::failure(
                "ZRYNA-D3102",
                "native syntax differs from the presealed source graph",
            ));
        }
        self.revalidate()?;
        let closure = VerifiedModuleClosure {
            entrypoint: self.entrypoint.clone(),
            sources: self.sources.clone(),
            syntax,
            modules: self.modules.clone(),
            edges: self.edges.clone(),
            graph_sha256: self.graph_v3,
        };
        Ok(NativeModuleSnapshot { source: self, closure })
    }

    /// Authenticates complete native M3 syntax and the presealed canonical ownership graph.
    /// # Errors
    /// Rejects stale capabilities, unsupported syntax and any final graph/source-map mismatch.
    pub fn verify_v4(self) -> Result<NativeOwnershipSnapshot<'root>, ModuleClosureError> {
        self.revalidate()?;
        let lexed = native_lexer::lex(&self.sources)
            .map_err(|error| ModuleClosureError::Rejected(vec![error.diagnostic().clone()]))?;
        let raw = native_parser::v4::parse_v4_candidate(&self.sources, &lexed)
            .map_err(|error| ModuleClosureError::Rejected(vec![error.diagnostic().clone()]))?;
        let closure = crate::ownership_closure::seal_native_closure(
            self.entrypoint.clone(),
            self.sources.clone(),
            raw,
            &self.modules,
            &self.edges,
            self.graph_v4,
        )?;
        self.revalidate()?;
        Ok(NativeOwnershipSnapshot { source: self, closure })
    }
}
