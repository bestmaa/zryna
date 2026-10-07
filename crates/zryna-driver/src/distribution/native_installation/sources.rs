//! Installation checkpoints around unchanged frozen package/source and syntax authorities.

use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use zryna_source::NormalizedSourcePath;

use super::NativeInstallation;
use crate::{
    ModuleClosureError, NativeModuleSnapshot, NativeOwnershipSnapshot, NativeSourceSnapshot,
    NativeSyntaxSnapshot, PackageResolutionRequest,
};

/// Private native source authority that retains its issuing installation borrow.
pub struct InstalledNativeSources<'installation> {
    installation: &'installation NativeInstallation,
    source: NativeSourceSnapshot<'static>,
}

enum Verified {
    V2(NativeSyntaxSnapshot<'static>),
    V3(NativeModuleSnapshot<'static>),
    V4(NativeOwnershipSnapshot<'static>),
}

/// Informational observation of a retained proof; this data issues no syntax/backend authority.
#[derive(Debug, serde::Serialize)]
pub struct InstalledNativeSummary {
    /// Exact existing syntax protocol selected by the private caller.
    pub protocol: u8,
    /// Original source paths and complete UTF-8 byte digests in deterministic path order.
    pub source_sha256: BTreeMap<String, String>,
    /// Existing canonical module graph digest for protocols v3/v4; v2 has no module graph.
    pub graph_sha256: Option<String>,
}

/// Opaque verified syntax bound to one retained private installation and source owner.
///
/// No raw snapshot, syntax, closure, unwrapping or cloning interface is provided. Observations
/// revalidate both authorities. Later production consumers and public selection are separate.
pub struct InstalledNativeSyntax<'installation> {
    installation: &'installation NativeInstallation,
    verified: Verified,
}

fn installation_changed(installation: &NativeInstallation) -> Result<(), ModuleClosureError> {
    installation.revalidate().map_err(|error| ModuleClosureError::Rejected(vec![error]))
}

impl NativeInstallation {
    /// Captures one exact admitted frozen package instance using the reviewed source issuer.
    ///
    /// # Errors
    /// Rejects stale installation, non-frozen resolution, wrong package identity and all existing
    /// no-follow source, import, lock, graph and resource failures. No compiler checkout is used.
    pub fn capture_package_sources(
        &self,
        request: &PackageResolutionRequest,
        package_id: &str,
        entrypoint: NormalizedSourcePath,
    ) -> Result<InstalledNativeSources<'_>, ModuleClosureError> {
        installation_changed(self)?;
        let captured = crate::capture_native_package_sources(request, package_id, entrypoint);
        installation_changed(self)?;
        let admitted = InstalledNativeSources { installation: self, source: captured? };
        admitted.revalidate()?;
        Ok(admitted)
    }
}

impl<'installation> InstalledNativeSources<'installation> {
    /// Revalidates both retained installation and original source identities.
    /// # Errors
    /// Rejects installation/source mutation, replacement or inconsistent graph/hash identity.
    pub fn revalidate(&self) -> Result<(), ModuleClosureError> {
        installation_changed(self.installation)?;
        self.source.revalidate()
    }

    /// Parses and verifies one exact existing syntax protocol without issuing backend authority.
    ///
    /// Installation revalidation also occurs on verification errors. The existing verifier
    /// consumes its source owner on error; this API claims no extra source checkpoint afterward.
    /// # Errors
    /// Rejects unsupported protocol selection, stale capabilities and existing syntax failures.
    pub fn verify(
        self,
        protocol: u8,
    ) -> Result<InstalledNativeSyntax<'installation>, ModuleClosureError> {
        self.revalidate()?;
        let verified = match protocol {
            2 => self.source.verify_v2().map(Verified::V2),
            3 => self.source.verify_v3().map(Verified::V3),
            4 => self.source.verify_v4().map(Verified::V4),
            _ => Err(ModuleClosureError::Rejected(vec![super::reject(
                "private native syntax protocol is unsupported",
            )])),
        };
        installation_changed(self.installation)?;
        let syntax = InstalledNativeSyntax { installation: self.installation, verified: verified? };
        syntax.revalidate()?;
        Ok(syntax)
    }
}

impl InstalledNativeSyntax<'_> {
    /// Revalidates installation plus the source capability retained by the versioned verifier.
    /// # Errors
    /// Rejects changed installation/source identities and existing closure inconsistency.
    pub fn revalidate(&self) -> Result<(), ModuleClosureError> {
        installation_changed(self.installation)?;
        match &self.verified {
            Verified::V2(value) => value.revalidate(),
            Verified::V3(value) => value.revalidate(),
            Verified::V4(value) => value.revalidate(),
        }
    }

    /// Observes original source/graph digests after both retained authorities are revalidated.
    ///
    /// Returned summaries cannot be used as verified syntax or installation authority.
    /// # Errors
    /// Rejects stale capabilities or an inconsistent original source-map binding.
    pub fn summary(&self) -> Result<InstalledNativeSummary, ModuleClosureError> {
        self.revalidate()?;
        let (protocol, sources, bound, graph_sha256) = match &self.verified {
            Verified::V2(value) => {
                (2, value.sources(), value.syntax().is_bound_to(value.sources()), None)
            }
            Verified::V3(value) => {
                let closure = value.closure();
                (
                    3,
                    closure.sources(),
                    closure.syntax().is_bound_to(closure.sources()),
                    Some(hex(closure.graph_sha256())),
                )
            }
            Verified::V4(value) => {
                let closure = value.closure();
                (
                    4,
                    closure.sources(),
                    closure.syntax().is_bound_to(closure.sources()),
                    Some(hex(closure.graph_sha256())),
                )
            }
        };
        if !bound {
            return Err(ModuleClosureError::Rejected(vec![super::reject(
                "private native syntax lost its original source binding",
            )]));
        }
        let mut source_sha256 = BTreeMap::new();
        for index in 0..sources.len() {
            let file = u32::try_from(index).map_err(|_| source_missing())?;
            let id = sources.verify_file_id(file).map_err(|_| source_missing())?;
            let source = sources.source(id).ok_or_else(source_missing)?;
            source_sha256.insert(
                source.path().as_str().to_owned(),
                format!("{:x}", Sha256::digest(source.text().as_bytes())),
            );
        }
        self.revalidate()?;
        Ok(InstalledNativeSummary { protocol, source_sha256, graph_sha256 })
    }

    /// Runs a caller-owned observation with installation/source checkpoints on either outcome.
    /// # Errors
    /// Propagates consumer errors and rejects installation/source changes during the callback.
    pub fn with_verified<R>(
        &self,
        consume: impl FnOnce(&Self) -> Result<R, ModuleClosureError>,
    ) -> Result<R, ModuleClosureError> {
        self.revalidate()?;
        let result = consume(self);
        self.revalidate()?;
        result
    }
}

fn hex(bytes: &[u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut digest = String::with_capacity(64);
    for byte in bytes {
        digest.push(char::from(DIGITS[usize::from(byte >> 4)]));
        digest.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    digest
}

fn source_missing() -> ModuleClosureError {
    ModuleClosureError::Rejected(vec![super::reject(
        "private native source identity is inconsistent",
    )])
}
