//! Versioned content bindings; neither an IR serialization nor execution/grant authority.

use super::invalid;
use crate::{WitSource, WitWorldAudit};
use sha2::{Digest, Sha256};
use zryna_diagnostics::Diagnostic;

const PROFILE: &[u8] = b"command-h1-v1";
const WORLD: &str = "zryna:capability-profiles/command@0.1.0";
const PARTITION: [u32; 7] = [256, 0, 65_536, 65_536, 15_728_640, 15_728_640, 16_777_216];

pub(super) struct Inputs<'a> {
    pub(super) source: [u8; 32],
    pub(super) language: [u8; 32],
    pub(super) storage: [u8; 32],
    pub(super) linear: [u8; 32],
    pub(super) linux: [u8; 32],
    pub(super) world: [u8; 32],
    pub(super) key: Option<&'a str>,
}

pub(super) fn program(inputs: &Inputs<'_>) -> Result<[u8; 32], Diagnostic> {
    Ok(Sha256::digest(program_bytes(inputs)?).into())
}

// Closed ordering: NUL-terminated domain, framed profile, six raw32 digests,
// pages plus three half-open partition pairs (u32LE), optional-key tag and frame.
fn program_bytes(inputs: &Inputs<'_>) -> Result<Vec<u8>, Diagnostic> {
    let mut bytes = b"zryna.command-program-binding.v1\0".to_vec();
    frame(&mut bytes, PROFILE)?;
    for digest in
        [inputs.source, inputs.language, inputs.storage, inputs.linear, inputs.linux, inputs.world]
    {
        bytes.extend_from_slice(&digest);
    }
    for field in PARTITION {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    match inputs.key {
        None => bytes.push(0),
        Some(key) => {
            bytes.push(1);
            frame(&mut bytes, key.as_bytes())?;
        }
    }
    Ok(bytes)
}

// WIT binding uses the same explicit framing and deterministic name order.
// The caller supplies only sources already authenticated by the exact34-file audit.
pub(super) fn world(sources: &[WitSource], audit: &WitWorldAudit) -> Result<[u8; 32], Diagnostic> {
    let mut bytes = b"zryna.command-wit-binding.v1\0".to_vec();
    frame(&mut bytes, WORLD.as_bytes())?;
    let mut sources = sources.iter().collect::<Vec<_>>();
    sources.sort_by_key(|source| source.path());
    count(&mut bytes, sources.len())?;
    for source in sources {
        frame(&mut bytes, source.path().as_bytes())?;
        bytes.extend_from_slice(&Sha256::digest(source.bytes()));
    }
    let world =
        audit.worlds().iter().find(|world| world.identity() == WORLD).ok_or_else(invalid)?;
    for names in [world.explicit_imports(), world.resolved_imports(), world.exports()] {
        count(&mut bytes, names.len())?;
        for name in names {
            frame(&mut bytes, name.as_bytes())?;
        }
    }
    Ok(Sha256::digest(bytes).into())
}

// Preserve the separately documented closure observation: sorted exact files,
// u64LE path length + path, then u64LE source length + full pinned source bytes.
pub(super) fn closure(sources: &[WitSource]) -> Result<[u8; 32], Diagnostic> {
    let mut digest = Sha256::new();
    let mut sources = sources.iter().collect::<Vec<_>>();
    sources.sort_by_key(|source| source.path());
    for source in sources {
        digest.update(u64::try_from(source.path().len()).map_err(|_| invalid())?.to_le_bytes());
        digest.update(source.path().as_bytes());
        digest.update(u64::try_from(source.bytes().len()).map_err(|_| invalid())?.to_le_bytes());
        digest.update(source.bytes());
    }
    Ok(digest.finalize().into())
}

fn frame(output: &mut Vec<u8>, value: &[u8]) -> Result<(), Diagnostic> {
    count(output, value.len())?;
    output.extend_from_slice(value);
    Ok(())
}

fn count(output: &mut Vec<u8>, value: usize) -> Result<(), Diagnostic> {
    output.extend_from_slice(&u32::try_from(value).map_err(|_| invalid())?.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests;
