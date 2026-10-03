//! One canonical serializer for the frozen M2/M3 module graph identities.

use sha2::{Digest as _, Sha256};
use zryna_source::NormalizedSourcePath;

use crate::{ModuleClosureError, ModuleEdge, ModuleRecord};

pub(crate) fn hash(
    domain: &[u8],
    entrypoint: &NormalizedSourcePath,
    modules: &[ModuleRecord],
    edges: &[ModuleEdge],
    invariant: fn() -> ModuleClosureError,
) -> Result<[u8; 32], ModuleClosureError> {
    let mut document = domain.to_vec();
    push_u32(&mut document, 1_usize, invariant)?;
    push_text(&mut document, entrypoint.as_str(), invariant)?;
    push_u32(&mut document, modules.len(), invariant)?;
    for module in modules {
        push_text(&mut document, module.path().as_str(), invariant)?;
        document.extend_from_slice(module.source_sha256());
    }
    push_u32(&mut document, edges.len(), invariant)?;
    for edge in edges {
        for value in [edge.importer().as_str(), edge.specifier(), edge.imported(), edge.local()] {
            push_text(&mut document, value, invariant)?;
        }
    }
    Ok(Sha256::digest(document).into())
}

fn push_text(
    document: &mut Vec<u8>,
    value: &str,
    invariant: fn() -> ModuleClosureError,
) -> Result<(), ModuleClosureError> {
    push_u32(document, value.len(), invariant)?;
    document.extend_from_slice(value.as_bytes());
    Ok(())
}

fn push_u32(
    document: &mut Vec<u8>,
    value: usize,
    invariant: fn() -> ModuleClosureError,
) -> Result<(), ModuleClosureError> {
    let value = u32::try_from(value).map_err(|_| invariant())?;
    document.extend_from_slice(&value.to_le_bytes());
    Ok(())
}
