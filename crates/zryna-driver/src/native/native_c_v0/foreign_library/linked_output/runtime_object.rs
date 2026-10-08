//! Structural audit of the original private runtime; libc names are not provider approval.

use super::{Diagnostic, HandleLinkRequirements};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;

mod inventory;
mod relocations;
mod tables;
#[cfg(test)]
mod tests;

const MAX_SECTIONS: usize = 16;
const MAX_SYMBOLS: usize = 4096;
const MAX_RELOCATIONS: usize = 32768;
// The checked uninstrumented runtime implementation uses exactly these four imports.
// This checks names only and cannot authenticate libc, its loader, or any host permission.
const IMPORTS: [&str; 4] = ["free", "malloc", "memcpy", "memset"];

pub(super) fn check(requirements: &HandleLinkRequirements, bytes: &[u8]) -> Result<(), Diagnostic> {
    if bytes.len() > zryna_backend_native::MAX_NATIVE_OBJECT_BYTES {
        return Err(limit());
    }
    let abi = requirements.object().program().source().runtime_abi();
    let source = requirements.private_runtime_source().ok_or_else(super::rejected)?;
    let source_digest: [u8; 32] = Sha256::digest(source).into();
    let header_digest: [u8; 32] = Sha256::digest(abi.native_linux_x86_64_header()).into();
    if Some(&source_digest) != requirements.private_runtime_source_sha256()
        || Some(&header_digest) != requirements.private_runtime_header_sha256()
    {
        return Err(super::rejected());
    }
    let expected = abi
        .native_linux_x86_64_functions()
        .map(zryna_ownership_runtime_abi::VerifiedNativeFunction::symbol)
        .collect::<BTreeSet<_>>();
    let table = tables::read(bytes)?;
    let symbols = inventory::check(&table, &expected)?;
    relocations::check(&table, &symbols)
}

fn error() -> Diagnostic {
    crate::native::native_error(
        "ZRYNA-C4104",
        "private runtime ELF object violates its original ABI and closed implementation inventory",
        "retain the original runtime issuer; supply strong exact definitions and bounded non-PIC relocations without undeclared imports, TLS or constructors",
    )
}

fn limit() -> Diagnostic {
    crate::native::native_error(
        "ZRYNA-C4105",
        "private runtime ELF object exceeds its bounded structural inventory",
        "use the existing object-byte limit and bounded section, symbol and relocation tables",
    )
}
