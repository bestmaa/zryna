//! Exact foreign-object capture. This seal is data, not operator approval or recipe permission.

use super::resource_identity::HandleLinkRequirements;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::Arc};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::contract::Direction;

mod audit;

/// Independently supplied actual inputs and expected artifact identity.
///
/// Expected hashes and dependency names are prerequisites to check, not authenticated operator
/// approval. A caller that changes both bytes and their expected hash creates different captured
/// data; it obtains no execution, cache, publication or OS-isolation authority.
pub struct ForeignLibraryInput<'a> {
    /// Exact versioned library id from the original declaration issuer.
    pub library_id: &'a str,
    /// Actual reviewed header bytes, compared byte-for-byte with the retained original.
    pub header_bytes: &'a [u8],
    /// Actual canonical operation policy bytes, compared with the retained original.
    pub policy_bytes: &'a [u8],
    /// Actual single static ELF relocatable object; no paths, archive search or loader lookup.
    pub object_bytes: &'a [u8],
    /// Independently expected acquired-object size.
    pub object_size: usize,
    /// Independently expected acquired-object SHA-256; distinct from declaration identity.
    pub object_sha256: &'a [u8; 32],
    /// Exact sorted, unique external dependency symbol inventory. No execution grant follows.
    pub dependency_symbols: &'a [&'a str],
}

/// Immutable source-bound foreign ELF snapshot with an independently checked closed inventory.
///
/// This cannot prove a C implementation obeys its ownership contract. Linking still requires an
/// authentic ordered plan, retained tools/sysroot/runtime objects and actual host authorization.
/// In particular, this type cannot satisfy the native-recipe admission boundary by itself.
///
/// The identity seal has no public field mutation:
/// ```compile_fail
/// use zryna_driver::native::native_c_v0::foreign_library::CapturedForeignLibrary;
/// fn replace_identity(mut captured: CapturedForeignLibrary) {
///     captured.object_sha256 = [0; 32];
/// }
/// ```
#[derive(Clone, Debug)]
pub struct CapturedForeignLibrary {
    requirements: HandleLinkRequirements,
    library_id: String,
    bytes: Arc<[u8]>,
    object_sha256: [u8; 32],
    definitions: BTreeSet<String>,
    dependencies: BTreeSet<String>,
}

impl CapturedForeignLibrary {
    /// Retained actual bytes; replacement of the caller's buffer/path cannot change them.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Exact acquired artifact hash, separate from source/declaration and header hashes.
    #[must_use]
    pub const fn object_sha256(&self) -> &[u8; 32] {
        &self.object_sha256
    }
    /// Exact original versioned library identity.
    #[must_use]
    pub fn library_id(&self) -> &str {
        &self.library_id
    }
    /// Original complete source/IR/MIR/object authority, never reconstructed from digests.
    #[must_use]
    pub const fn requirements(&self) -> &HandleLinkRequirements {
        &self.requirements
    }
    /// Exact strong function definitions, including the library's unselected declared operations.
    pub fn definitions(&self) -> impl ExactSizeIterator<Item = &str> {
        self.definitions.iter().map(String::as_str)
    }
    /// Actual undefined symbols, equal to the supplied closed dependency inventory.
    pub fn dependencies(&self) -> impl ExactSizeIterator<Item = &str> {
        self.dependencies.iter().map(String::as_str)
    }
    /// Rejects reuse with a changed source, selected machine object or declaration issuer.
    /// # Errors
    /// Returns identity failure before a linker or foreign call can consume mismatched inputs.
    pub fn check_binding(&self, requirements: &HandleLinkRequirements) -> Result<(), Diagnostic> {
        if requirements.object_sha256() != self.requirements.object_sha256()
            || requirements.declaration_sha256() != self.requirements.declaration_sha256()
            || requirements.object().program().source().source_map_identity()
                != self.requirements.object().program().source().source_map_identity()
        {
            return Err(identity_error());
        }
        Ok(())
    }
}

/// Captures one actual ELF object against genuine original declaration and machine authority.
///
/// Full library definitions are checked, not only currently selected calls. Matching names or
/// hashes alone cannot establish C body safety, host enforcement or recipe execution permission.
/// # Errors
/// Rejects substituted material, unneeded libraries, unsupported ELF, open inventories and limits.
pub fn capture_foreign_library(
    requirements: &HandleLinkRequirements,
    input: &ForeignLibraryInput<'_>,
) -> Result<CapturedForeignLibrary, Diagnostic> {
    if input.object_bytes.len() > zryna_backend_native::MAX_NATIVE_OBJECT_BYTES {
        return Err(audit::limit_error());
    }
    let authority = requirements
        .object()
        .program()
        .source()
        .private_authority()
        .body_authority()
        .declaration_authority();
    if !requirements.libraries().iter().any(|library| library.id() == input.library_id)
        || authority.header_bytes(input.library_id) != Some(input.header_bytes)
        || authority.policy_bytes(input.library_id) != Some(input.policy_bytes)
        || input.object_size != input.object_bytes.len()
        || &<[u8; 32]>::from(Sha256::digest(input.object_bytes)) != input.object_sha256
    {
        return Err(identity_error());
    }
    let definitions = requirements
        .object()
        .program()
        .operations()
        .map(zryna_native_mir::native_c_v0::VerifiedOperation::declaration)
        .filter(|declaration| {
            declaration.direction == Direction::Import && declaration.library == input.library_id
        })
        .map(|declaration| declaration.symbol.clone())
        .collect::<BTreeSet<_>>();
    let dependencies = audit::check(input.object_bytes, &definitions, input.dependency_symbols)?;
    Ok(CapturedForeignLibrary {
        requirements: requirements.clone(),
        library_id: input.library_id.to_owned(),
        bytes: Arc::from(input.object_bytes),
        object_sha256: *input.object_sha256,
        definitions,
        dependencies,
    })
}

fn identity_error() -> Diagnostic {
    super::super::native_error(
        "ZRYNA-C4102",
        "foreign ELF inputs do not match the retained source and artifact identity",
        "retain exact original library, header, policy and independently expected object bytes",
    )
}

#[cfg(test)]
mod tests;
