//! Fail-closed policy over an owned, bounded self-relative security descriptor snapshot.

use std::io;

const MAX_SECURITY_BYTES: usize = 65_536;
const MAX_ACES: usize = 256;
const LOCAL_SYSTEM: &[u8] = &[1, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0];

pub(crate) fn verify(descriptor: &[u8], user: &[u8]) -> io::Result<()> {
    if descriptor.len() < 20
        || descriptor.len() > MAX_SECURITY_BYTES
        || descriptor[0] != 1
        || sid(user)? != user
    {
        return Err(rejected());
    }
    let control = word(descriptor, 2)?;
    if control & 0x8004 != 0x8004 {
        return Err(rejected()); // Self-relative and a present, non-null DACL are mandatory.
    }
    let owner_offset = offset(descriptor, 4)?;
    let owner = sid(descriptor.get(owner_offset..).ok_or_else(rejected)?)?;
    if owner != user {
        return Err(rejected());
    }
    let acl_offset = offset(descriptor, 16)?;
    let acl = descriptor.get(acl_offset..).ok_or_else(rejected)?;
    if acl.len() < 8 || !matches!(acl[0], 2 | 4) {
        return Err(rejected());
    }
    let acl_length = usize::from(word(acl, 2)?);
    let acl = acl.get(..acl_length).filter(|value| value.len() >= 8).ok_or_else(rejected)?;
    if owner_offset < acl_offset + acl_length && acl_offset < owner_offset + owner.len() {
        return Err(rejected());
    }
    let count = usize::from(word(acl, 4)?);
    if count > MAX_ACES {
        return Err(rejected());
    }
    let mut cursor = 8;
    for _ in 0..count {
        let tail = acl.get(cursor..).filter(|value| value.len() >= 8).ok_or_else(rejected)?;
        let size = usize::from(word(tail, 2)?);
        if size < 16 || size % 4 != 0 || !matches!(tail[0], 0 | 1) || tail[1] & !0x1f != 0 {
            return Err(rejected()); // Object/callback/conditional and unknown ACEs reject.
        }
        let ace = tail.get(..size).ok_or_else(rejected)?;
        let trustee = sid(&ace[8..])?;
        if trustee.len() + 8 != size {
            return Err(rejected());
        }
        let mask = dword(ace, 4)?;
        let inherit_only = ace[1] & 8 != 0;
        if ace[0] == 0 && !inherit_only && mask != 0 && trustee != user && trustee != LOCAL_SYSTEM {
            return Err(rejected());
        }
        cursor = cursor.checked_add(size).ok_or_else(rejected)?;
    }
    if acl[cursor..].iter().any(|byte| *byte != 0) {
        return Err(rejected());
    }
    Ok(())
}

pub(crate) fn sid(bytes: &[u8]) -> io::Result<&[u8]> {
    if bytes.len() < 8 || bytes[0] != 1 || bytes[1] > 15 {
        return Err(rejected());
    }
    let length = 8 + usize::from(bytes[1]) * 4;
    bytes.get(..length).ok_or_else(rejected)
}

fn offset(bytes: &[u8], at: usize) -> io::Result<usize> {
    let offset = usize::try_from(dword(bytes, at)?).map_err(|_| rejected())?;
    if offset < 20 || offset % 4 != 0 { Err(rejected()) } else { Ok(offset) }
}

fn word(bytes: &[u8], at: usize) -> io::Result<u16> {
    let raw = bytes.get(at..at + 2).ok_or_else(rejected)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn dword(bytes: &[u8], at: usize) -> io::Result<u32> {
    let raw = bytes.get(at..at + 4).ok_or_else(rejected)?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

pub(crate) fn rejected() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "private grant-file ownership or DACL could not be proved",
    )
}

#[cfg(test)]
mod tests;
