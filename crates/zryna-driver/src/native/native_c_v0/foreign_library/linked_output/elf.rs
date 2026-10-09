//! Bounded observations from loadable tables, not a loader admission policy.
use super::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(super) struct Dependencies {
    pub(super) interpreter: Option<Vec<u8>>,
    pub(super) needed: Vec<Vec<u8>>,
    search_tags: Vec<(u64, Vec<u8>)>,
    dynamic_entries: Vec<(u64, u64)>,
    version_sections: Vec<(u32, Vec<u8>)>,
}

fn range(bytes: &[u8], offset: u64, size: u64) -> Result<&[u8], Diagnostic> {
    let start = usize::try_from(offset).map_err(|_| rejected())?;
    let length = usize::try_from(size).map_err(|_| rejected())?;
    bytes.get(start..start.checked_add(length).ok_or_else(rejected)?).ok_or_else(rejected)
}
fn word(bytes: &[u8], offset: usize, size: usize) -> Result<u64, Diagnostic> {
    let data = range(bytes, offset as u64, size as u64)?;
    Ok(data.iter().enumerate().fold(0, |value, (i, byte)| value | (u64::from(*byte) << (i * 8))))
}
fn string(bytes: &[u8], offset: u64) -> Result<Vec<u8>, Diagnostic> {
    let start = usize::try_from(offset).map_err(|_| rejected())?;
    let data = bytes.get(start..).ok_or_else(rejected)?;
    let end = data.iter().position(|byte| *byte == 0).ok_or_else(rejected)?;
    if end == 0 {
        return Err(rejected());
    }
    Ok(data[..end].to_vec())
}

pub(super) fn read(bytes: &[u8]) -> Result<Dependencies, Diagnostic> {
    if bytes.len() > native::MAX_NATIVE_EXECUTABLE_BYTES
        || bytes.get(..7) != Some(b"\x7fELF\x02\x01\x01")
        || word(bytes, 16, 2)? != 2
        || word(bytes, 18, 2)? != 62
        || word(bytes, 52, 2)? != 64
    {
        return Err(rejected());
    }
    let phnum = word(bytes, 56, 2)?;
    if word(bytes, 54, 2)? != 56 || phnum == 0 || phnum == 65535 {
        return Err(rejected());
    }
    let table = range(bytes, word(bytes, 32, 8)?, phnum.checked_mul(56).ok_or_else(rejected)?)?;
    let mut loads = Vec::new();
    let mut interpreter = None;
    let mut dynamic = None;
    for row in table.chunks_exact(56) {
        let kind = word(row, 0, 4)?;
        let offset = word(row, 8, 8)?;
        let address = word(row, 16, 8)?;
        let size = word(row, 32, 8)?;
        if kind == 1 {
            loads.push((address, offset, size));
        }
        if kind == 2 {
            if dynamic.is_some()
                || size > native::MAX_NATIVE_TOOL_OUTPUT_BYTES as u64
                || size % 16 != 0
            {
                return Err(rejected());
            }
            dynamic = Some(range(bytes, offset, size)?);
        }
        if kind == 3 {
            if interpreter.is_some() || size > native::MAX_NATIVE_TOOL_OUTPUT_BYTES as u64 {
                return Err(rejected());
            }
            let data = range(bytes, offset, size)?;
            let name = string(data, 0)?;
            if name.len() + 1 != data.len() {
                return Err(rejected());
            }
            interpreter = Some(name);
        }
    }
    let mut result = read_dynamic(bytes, dynamic, &loads)?;
    result.interpreter = interpreter;
    result.version_sections = read_versions(bytes)?;
    Ok(result)
}

fn read_dynamic(
    bytes: &[u8],
    dynamic: Option<&[u8]>,
    loads: &[(u64, u64, u64)],
) -> Result<Dependencies, Diagnostic> {
    let mut entries = Vec::new();
    let mut strings_address = None;
    let mut strings_size = None;
    let mut terminated = false;
    if let Some(dynamic) = dynamic {
        for row in dynamic.chunks_exact(16) {
            let tag = word(row, 0, 8)?;
            let value = word(row, 8, 8)?;
            if terminated && (tag != 0 || value != 0) {
                return Err(rejected());
            }
            if tag == 0 {
                terminated = true;
            }
            if tag == 5 && strings_address.replace(value).is_some() {
                return Err(rejected());
            }
            if tag == 10 && strings_size.replace(value).is_some() {
                return Err(rejected());
            }
            entries.push((tag, value));
        }
        if !terminated {
            return Err(rejected());
        }
    }
    let mut needed = Vec::new();
    let mut search_tags = Vec::new();
    if let (Some(address), Some(size)) = (strings_address, strings_size) {
        if size > native::MAX_NATIVE_TOOL_OUTPUT_BYTES as u64 {
            return Err(rejected());
        }
        let mut matching = Vec::new();
        for &(base, offset, load_size) in loads {
            if let Some(delta) = address.checked_sub(base)
                && delta <= load_size
                && size <= load_size - delta
            {
                matching.push(range(bytes, offset.checked_add(delta).ok_or_else(rejected)?, size)?);
            }
        }
        if matching.len() != 1 {
            return Err(rejected());
        }
        let mut total = 0_usize;
        for (tag, offset) in &entries {
            if [1, 15, 29].contains(tag) {
                let value = string(matching[0], *offset)?;
                total = total.checked_add(value.len()).ok_or_else(rejected)?;
                if total > native::MAX_NATIVE_TOOL_OUTPUT_BYTES {
                    return Err(rejected());
                }
                if *tag == 1 {
                    needed.push(value);
                } else {
                    search_tags.push((*tag, value));
                }
            }
        }
    } else if strings_address.is_some()
        || strings_size.is_some()
        || entries.iter().any(|(tag, _)| [1, 15, 29].contains(tag))
    {
        return Err(rejected());
    }
    Ok(Dependencies {
        interpreter: None,
        needed,
        search_tags,
        dynamic_entries: entries,
        version_sections: Vec::new(),
    })
}

fn read_versions(bytes: &[u8]) -> Result<Vec<(u32, Vec<u8>)>, Diagnostic> {
    // Retain raw GNU version section data; no version-to-provider compatibility is inferred.
    let mut versions = Vec::new();
    let count = word(bytes, 60, 2)?;
    if count == 0 || word(bytes, 58, 2)? != 64 {
        return Err(rejected());
    }
    let sections = range(bytes, word(bytes, 40, 8)?, count.checked_mul(64).ok_or_else(rejected)?)?;
    let mut total = 0;
    for row in sections.chunks_exact(64) {
        let kind = word(row, 4, 4)?;
        if [0x6fff_ffff, 0x6fff_fffe, 0x6fff_fffd].contains(&kind) {
            let data = range(bytes, word(row, 24, 8)?, word(row, 32, 8)?)?;
            total += data.len();
            if total > native::MAX_NATIVE_TOOL_OUTPUT_BYTES {
                return Err(rejected());
            }
            versions.push((u32::try_from(kind).map_err(|_| rejected())?, data.to_vec()));
        }
    }
    Ok(versions)
}
