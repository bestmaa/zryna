//! Independent source-bound foreign declaration authority, without body or executable authority.
//!
//! Captured bytes and complete independently authenticated foreign syntax are checked here.
//! Library promises remain trusted C contracts; digest agreement does not prove arbitrary C code.
//! Declaration verification produces no typed body or executable authority. The separate body
//! module derives a typed conditional foreign flow plan, without IR/MIR, linking or a selector.

use std::fmt;
use zryna_source::{SourceMap, SourceMapIdentity, Span};
use zryna_syntax::{
    native_c_source_v0::AuthenticatedForeignSources,
    native_c_v0::{self, raw},
};

mod identity;
mod policy;
mod resources;
mod source_bindings;

pub mod body;

/// Maximum bytes in one explicitly captured header or policy document.
pub const MAX_MATERIAL_BYTES: usize = 1_048_576;

/// Explicitly captured library material; fields alone supply no declaration authority.
#[derive(Clone, Copy, Debug)]
pub struct LibraryMaterial<'a> {
    /// Exact reviewed library id; never a filesystem path.
    pub library_id: &'a str,
    /// Complete original header bytes, captured independently of the sidecar.
    pub header_bytes: &'a [u8],
    /// Complete canonical library policy bytes, captured independently of the sidecar.
    pub policy_bytes: &'a [u8],
}

#[derive(Clone, Debug)]
struct CapturedLibrary {
    id: String,
    header: Vec<u8>,
    policy: Vec<u8>,
}

/// Opaque verified declaration set; it is deliberately not an executable body authority.
#[derive(Clone, Debug)]
pub struct VerifiedDeclarationSet {
    declarations: raw::DeclarationSet,
    syntax: AuthenticatedForeignSources,
    spans: Vec<Span>,
    wire: Vec<u8>,
    digest: [u8; 32],
    libraries: Vec<CapturedLibrary>,
}

impl VerifiedDeclarationSet {
    /// Exact declaration-domain SHA-256 computed from the complete canonical wire.
    #[must_use]
    pub const fn declaration_sha256(&self) -> &[u8; 32] {
        &self.digest
    }
    /// Exact immutable captured source-map identity.
    #[must_use]
    pub const fn source_map_identity(&self) -> SourceMapIdentity {
        self.syntax.source_map_identity()
    }
    /// Checks the original immutable map, not a rebuilt map with matching bytes.
    #[must_use]
    pub fn belongs_to(&self, sources: &SourceMap) -> bool {
        self.syntax.belongs_to(sources)
    }
    /// Complete verified declaration count.
    #[must_use]
    pub fn operation_count(&self) -> usize {
        self.declarations.operations.len()
    }
    /// Complete retained declaration bytes; re-decoding them produces only untrusted records.
    #[must_use]
    pub fn declaration_bytes(&self) -> &[u8] {
        &self.wire
    }
    /// Exact captured header bytes for an authenticated library.
    #[must_use]
    pub fn header_bytes(&self, library: &str) -> Option<&[u8]> {
        self.libraries.iter().find(|item| item.id == library).map(|item| item.header.as_slice())
    }
    /// Exact captured canonical policy bytes for an authenticated library.
    #[must_use]
    pub fn policy_bytes(&self, library: &str) -> Option<&[u8]> {
        self.libraries.iter().find(|item| item.id == library).map(|item| item.policy.as_slice())
    }
    /// Looks up an exact declaration view, preserving distinct declared C spellings.
    #[must_use]
    pub fn operation(&self, key: &str) -> Option<VerifiedOperation<'_>> {
        self.declarations.operations.iter().position(|operation| operation.key == key).map(
            |index| VerifiedOperation {
                operation: &self.declarations.operations[index],
                span: self.spans[index],
            },
        )
    }
}

/// Read-only declaration view; no raw record can construct this value.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedOperation<'a> {
    operation: &'a raw::Operation,
    span: Span,
}

impl VerifiedOperation<'_> {
    /// Exact canonical operation key.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.operation.key
    }
    /// Exact source-map-issued declaration or binding call span.
    #[must_use]
    pub const fn source_span(&self) -> Span {
        self.span
    }
    /// Ordered exact C argument spellings; `c-int` remains distinct from `c-i32`.
    #[must_use]
    pub fn parameter_carriers(&self) -> impl ExactSizeIterator<Item = raw::AbiType> + '_ {
        self.operation.parameters.iter().map(|parameter| parameter.abi)
    }
    /// Exact C result spelling.
    #[must_use]
    pub const fn result_carrier(&self) -> raw::AbiType {
        self.operation.result
    }
}

/// Single fail-closed declaration rejection; no partial set or executable authority is returned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationError {
    code: &'static str,
    detail: &'static str,
}
impl DeclarationError {
    /// Fixed producing diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }
    /// Rejected category or exact budget metric.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}
impl fmt::Display for DeclarationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.code, self.detail)
    }
}
impl std::error::Error for DeclarationError {}

fn require(
    condition: bool,
    code: &'static str,
    detail: &'static str,
) -> Result<(), DeclarationError> {
    if condition { Ok(()) } else { Err(DeclarationError { code, detail }) }
}

/// Authenticates complete declarations against explicit captures and independent syntax.
///
/// The caller must supply the exact original immutable source map and complete library material
/// set. This function never opens files or accepts a digest callback, AST claim or predecessor
/// execution seal. It verifies declaration signatures/policies and their exact parsed source
/// correspondence, including all intrinsic sites and exports. It does not type check private
/// bodies, prove ownership/status dominance/cleanup, inspect C objects or authorize execution.
///
/// # Errors
/// Rejects bounded wire/shape, target, identity, policy/resource contradictions or parsed source
/// mismatches, retaining no partially verified declaration set.
pub fn verify(
    declaration_bytes: &[u8],
    sources: &SourceMap,
    syntax: &AuthenticatedForeignSources,
    materials: &[LibraryMaterial<'_>],
    selected_target: &str,
) -> Result<VerifiedDeclarationSet, DeclarationError> {
    let declarations = native_c_v0::decode(declaration_bytes, selected_target)
        .map_err(|failure| DeclarationError { code: failure.code(), detail: failure.detail() })?;
    require(syntax.belongs_to(sources), "ZRYNA-C4106", "source-map-identity")?;
    identity::check(&declarations, materials)?;
    policy::check(&declarations)?;
    resources::check(&declarations)?;
    let spans = source_bindings::check(&declarations, syntax)?;
    Ok(VerifiedDeclarationSet {
        declarations,
        syntax: syntax.clone(),
        spans,
        wire: declaration_bytes.to_vec(),
        digest: identity::declaration_digest(declaration_bytes),
        libraries: materials
            .iter()
            .map(|material| CapturedLibrary {
                id: material.library_id.to_owned(),
                header: material.header_bytes.to_vec(),
                policy: material.policy_bytes.to_vec(),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests;
