//! Exact immutable link-input requirements, without foreign artifact or host authorization.

use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use zryna_backend_native::native_c_v0::resources::ValidatedHandleEntries;

/// Captured library bytes required by the original declaration authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryRequirement {
    id: String,
    header_sha256: [u8; 32],
    policy_sha256: [u8; 32],
}
impl LibraryRequirement {
    /// Exact authenticated library identity, including its version.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Hash of the exact retained C header bytes, rather than a path or filename.
    #[must_use]
    pub const fn header_sha256(&self) -> &[u8; 32] {
        &self.header_sha256
    }
    /// Hash of the exact retained canonical ownership and operation policy bytes.
    #[must_use]
    pub const fn policy_sha256(&self) -> &[u8; 32] {
        &self.policy_sha256
    }
}

/// Input-bound private requirement record. Reconstructed digests cannot construct this record.
///
/// This retains the genuine source/machine/object issuer. It neither acquires nor authorizes a
/// foreign object, supplies an OS isolation proof, nor grants recipe execution. A future recipe
/// admission boundary must independently bind its actual material and host proof to these inputs.
#[derive(Clone, Debug)]
pub struct HandleLinkRequirements {
    object: ValidatedHandleEntries,
    object_sha256: [u8; 32],
    private_header_sha256: [u8; 32],
    declaration_sha256: [u8; 32],
    libraries: Vec<LibraryRequirement>,
    private_runtime_source: Option<Vec<u8>>,
    private_runtime_source_sha256: Option<[u8; 32]>,
    private_runtime_header_sha256: Option<[u8; 32]>,
}
impl HandleLinkRequirements {
    /// Original audited object, exact selected entries/import symbols and complete machine seal.
    #[must_use]
    pub const fn object(&self) -> &ValidatedHandleEntries {
        &self.object
    }
    /// Exact relocatable byte digest, including the imported call relocation inventory.
    #[must_use]
    pub const fn object_sha256(&self) -> &[u8; 32] {
        &self.object_sha256
    }
    /// Exact generated compiler-private context/input/outcome header digest.
    #[must_use]
    pub const fn private_header_sha256(&self) -> &[u8; 32] {
        &self.private_header_sha256
    }
    /// Declaration-domain hash from the original issuer, preserving its established domain.
    #[must_use]
    pub const fn declaration_sha256(&self) -> &[u8; 32] {
        &self.declaration_sha256
    }
    /// Exact captured libraries needed by selected calls and reverse terminal cleanup.
    #[must_use]
    pub fn libraries(&self) -> &[LibraryRequirement] {
        &self.libraries
    }
    /// Exact required checked private runtime source, retaining its original ABI/layout issuer.
    /// Source provenance is not a runtime-object audit or native execution permission.
    #[must_use]
    pub fn private_runtime_source(&self) -> Option<&[u8]> {
        self.private_runtime_source.as_deref()
    }
    /// Digest of the rendered checked private runtime implementation and sealed element cases.
    #[must_use]
    pub const fn private_runtime_source_sha256(&self) -> Option<&[u8; 32]> {
        self.private_runtime_source_sha256.as_ref()
    }
    /// Digest of the exact checked runtime header retained by the branded declaration issuer.
    #[must_use]
    pub const fn private_runtime_header_sha256(&self) -> Option<&[u8; 32]> {
        self.private_runtime_header_sha256.as_ref()
    }
}

/// Binds exact private linking requirements to the already independently audited handle artifact.
///
/// Every foreign operation remains available through the retained artifact, including exact key,
/// symbol, signature, resource kind, allocator, release identity and reviewed guarantees. Hashes
/// identify required inputs; they never replace that authority or an execution permission.
/// # Errors
/// Rejects a missing captured header/policy instead of constructing a partial requirement record.
pub fn handle_link_requirements(
    object: &ValidatedHandleEntries,
) -> Result<HandleLinkRequirements, zryna_diagnostics::Diagnostic> {
    let authority =
        object.program().source().private_authority().body_authority().declaration_authority();
    let required = object
        .imported_operations()
        .map(|operation| operation.declaration().library.as_str())
        .collect::<BTreeSet<_>>();
    let mut libraries = Vec::with_capacity(required.len());
    for id in required {
        let header = authority.header_bytes(id).ok_or_else(missing_material)?;
        let policy = authority.policy_bytes(id).ok_or_else(missing_material)?;
        libraries.push(LibraryRequirement {
            id: id.to_owned(),
            header_sha256: Sha256::digest(header).into(),
            policy_sha256: Sha256::digest(policy).into(),
        });
    }
    let private_runtime_source = object
        .imported_runtime_operations()
        .next()
        .map(|_| crate::ownership_runtime_v1::render_native_c_source(object.program()));
    let private_runtime_source_sha256 =
        private_runtime_source.as_ref().map(|source| Sha256::digest(source).into());
    let private_runtime_header_sha256 = private_runtime_source.as_ref().map(|_| {
        Sha256::digest(object.program().source().runtime_abi().native_linux_x86_64_header()).into()
    });
    Ok(HandleLinkRequirements {
        object: object.clone(),
        object_sha256: Sha256::digest(object.bytes()).into(),
        private_header_sha256: Sha256::digest(object.header().as_bytes()).into(),
        declaration_sha256: *authority.declaration_sha256(),
        libraries,
        private_runtime_source,
        private_runtime_source_sha256,
        private_runtime_header_sha256,
    })
}

fn missing_material() -> zryna_diagnostics::Diagnostic {
    super::super::native_error(
        "ZRYNA-C4104",
        "native C handle linking requirements lack exact retained library material",
        "retain the original declaration, header and policy issuer",
    )
}
