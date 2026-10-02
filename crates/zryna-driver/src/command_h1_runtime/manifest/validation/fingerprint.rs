//! Content consistency is separate from authenticating the original compiler or execution.

use super::super::{Document, PROFILE, WORLD, hex, invalid};
use sha2::{Digest, Sha256};
use zryna_backend_webassembly::{WitSource, audit_pinned_wit_worlds, pinned_wit_sources};
use zryna_diagnostics::Diagnostic;

pub(super) fn digest(text: &str) -> Result<[u8; 32], Diagnostic> {
    if text.len() != 64
        || !text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid());
    }
    let mut bytes = [0; 32];
    for (byte, pair) in bytes.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
        let pair = std::str::from_utf8(pair).map_err(|_| invalid())?;
        *byte = u8::from_str_radix(pair, 16).map_err(|_| invalid())?;
    }
    Ok(bytes)
}

pub(super) fn validate(document: &Document) -> Result<(), Diagnostic> {
    let component = &document.component;
    let mut sources = pinned_wit_sources();
    sources.sort_by(|left, right| left.path().cmp(right.path()));
    let audit = audit_pinned_wit_worlds(&sources).map_err(|_| invalid())?;
    let world =
        audit.worlds().iter().find(|world| world.identity() == WORLD).ok_or_else(invalid)?;
    if component.packages != audit.packages()
        || component.explicit_imports != world.explicit_imports()
        || component.resolved_imports != world.resolved_imports()
        || component.exports != world.exports()
    {
        return Err(invalid());
    }
    let mut world_bytes = b"zryna.command-wit-binding.v1\0".to_vec();
    frame(&mut world_bytes, WORLD.as_bytes())?;
    count(&mut world_bytes, sources.len())?;
    for source in &sources {
        frame(&mut world_bytes, source.path().as_bytes())?;
        world_bytes.extend_from_slice(&Sha256::digest(source.bytes()));
    }
    for names in [world.explicit_imports(), world.resolved_imports(), world.exports()] {
        count(&mut world_bytes, names.len())?;
        for name in names {
            frame(&mut world_bytes, name.as_bytes())?;
        }
    }
    let world_digest: [u8; 32] = Sha256::digest(world_bytes).into();
    if component.world_sha256 != hex(&world_digest)
        || component.wit_closure_digest != closure(&sources)?
    {
        return Err(invalid());
    }
    let mut program = b"zryna.command-program-binding.v1\0".to_vec();
    frame(&mut program, PROFILE.as_bytes())?;
    for text in [
        &document.source.sha256,
        &component.language_sha256,
        &component.storage_sha256,
        &component.linear32_sha256,
        &component.linux_x86_64_sha256,
        &component.world_sha256,
    ] {
        program.extend_from_slice(&digest(text)?);
    }
    for number in [256_u32, 0, 65_536, 65_536, 15_728_640, 15_728_640, 16_777_216] {
        program.extend_from_slice(&number.to_le_bytes());
    }
    match document.source.requirements.first() {
        None => program.push(0),
        Some(grant) => {
            program.push(1);
            frame(&mut program, grant.key.as_bytes())?;
        }
    }
    let program_digest: [u8; 32] = Sha256::digest(program).into();
    if document.source.program_binding != hex(&program_digest) {
        return Err(invalid());
    }
    Ok(())
}

fn closure(sources: &[WitSource]) -> Result<String, Diagnostic> {
    let mut hash = Sha256::new();
    for source in sources {
        hash.update(u64::try_from(source.path().len()).map_err(|_| invalid())?.to_le_bytes());
        hash.update(source.path().as_bytes());
        hash.update(u64::try_from(source.bytes().len()).map_err(|_| invalid())?.to_le_bytes());
        hash.update(source.bytes());
    }
    Ok(hex(&hash.finalize().into()))
}

fn frame(bytes: &mut Vec<u8>, text: &[u8]) -> Result<(), Diagnostic> {
    count(bytes, text.len())?;
    bytes.extend_from_slice(text);
    Ok(())
}

fn count(bytes: &mut Vec<u8>, number: usize) -> Result<(), Diagnostic> {
    bytes.extend_from_slice(&u32::try_from(number).map_err(|_| invalid())?.to_le_bytes());
    Ok(())
}
