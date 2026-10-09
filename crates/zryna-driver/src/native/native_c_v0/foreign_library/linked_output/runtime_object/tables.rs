//! Validate raw ELF ranges, section roles and record framing before any record iteration.

use super::{Diagnostic, MAX_RELOCATIONS, MAX_SECTIONS, MAX_SYMBOLS, error, limit};
use std::collections::BTreeSet;

pub(super) struct Section<'a> {
    pub(super) name: &'a str,
    pub(super) size: u64,
    pub(super) data: &'a [u8],
}

pub(super) struct Tables<'a> {
    pub(super) sections: Vec<Section<'a>>,
    pub(super) symbols: &'a [u8],
    pub(super) strings: &'a [u8],
    pub(super) first_global: usize,
    pub(super) text_index: usize,
    pub(super) relocations: &'a [u8],
}

pub(super) fn number(bytes: &[u8], offset: usize, width: usize) -> Result<u64, Diagnostic> {
    let data = bytes.get(offset..offset.checked_add(width).ok_or_else(error)?).ok_or_else(error)?;
    let mut word = [0; 8];
    word.get_mut(..width).ok_or_else(error)?.copy_from_slice(data);
    Ok(u64::from_le_bytes(word))
}

fn index(value: u64) -> Result<usize, Diagnostic> {
    usize::try_from(value).map_err(|_| error())
}

fn range(bytes: &[u8], offset: u64, size: u64) -> Result<&[u8], Diagnostic> {
    bytes.get(index(offset)?..index(offset.checked_add(size).ok_or_else(error)?)?).ok_or_else(error)
}

pub(super) fn string(bytes: &[u8], offset: u64) -> Result<&str, Diagnostic> {
    let tail = bytes.get(index(offset)?..).ok_or_else(error)?;
    let length = tail.iter().position(|byte| *byte == 0).ok_or_else(error)?;
    if length > 128 {
        return Err(error());
    }
    std::str::from_utf8(&tail[..length]).map_err(|_| error())
}

pub(super) fn read(bytes: &[u8]) -> Result<Tables<'_>, Diagnostic> {
    if bytes.get(..16) != Some(&[0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0])
        || number(bytes, 16, 2)? != 1
        || number(bytes, 18, 2)? != 62
        || number(bytes, 20, 4)? != 1
        || number(bytes, 24, 8)? != 0
        || number(bytes, 32, 8)? != 0
        || number(bytes, 48, 4)? != 0
        || number(bytes, 52, 2)? != 64
        || number(bytes, 54, 2)? != 0
        || number(bytes, 56, 2)? != 0
        || number(bytes, 58, 2)? != 64
    {
        return Err(error());
    }
    let count = index(number(bytes, 60, 2)?)?;
    if count > MAX_SECTIONS + 1 {
        return Err(limit());
    }
    let offset = number(bytes, 40, 8)?;
    if count < 2 || offset < 64 || offset % 8 != 0 {
        return Err(error());
    }
    let headers = range(bytes, offset, (count * 64) as u64)?;
    if headers[..64].iter().any(|byte| *byte != 0) {
        return Err(error());
    }
    let names_index = index(number(bytes, 62, 2)?)?;
    let name_header = headers.get(names_index * 64..(names_index + 1) * 64).ok_or_else(error)?;
    let names = range(bytes, number(name_header, 24, 8)?, number(name_header, 32, 8)?)?;
    let sections = read_sections(bytes, headers, names, offset)?;
    if sections[names_index].name != ".shstrtab" {
        return Err(error());
    }
    let find = |name| sections.iter().position(|section| section.name == name).ok_or_else(error);
    let text_index = find(".text")?;
    find(".note.GNU-stack")?;
    let symbol_index = find(".symtab")?;
    let symbol_header = &headers[symbol_index * 64..(symbol_index + 1) * 64];
    let symbols = sections[symbol_index].data;
    let count = symbols.len() / 24;
    let first_global = index(number(symbol_header, 44, 4)?)?;
    if count > MAX_SYMBOLS + 1 {
        return Err(limit());
    }
    if count == 0
        || symbols.len() % 24 != 0
        || symbols[..24].iter().any(|byte| *byte != 0)
        || first_global == 0
        || first_global > count
        || number(symbol_header, 56, 8)? != 24
        || index(number(symbol_header, 40, 4)?)? != find(".strtab")?
    {
        return Err(error());
    }
    let strings = sections[find(".strtab")?].data;
    let relocations =
        if let Some(i) = sections.iter().position(|section| section.name == ".rela.text") {
            let header = &headers[i * 64..(i + 1) * 64];
            let rows = sections[i].data;
            if rows.len() / 24 > MAX_RELOCATIONS {
                return Err(limit());
            }
            if rows.len() % 24 != 0
                || number(header, 56, 8)? != 24
                || index(number(header, 40, 4)?)? != symbol_index
                || index(number(header, 44, 4)?)? != text_index
            {
                return Err(error());
            }
            rows
        } else {
            &[]
        };
    Ok(Tables { sections, symbols, strings, first_global, text_index, relocations })
}

fn read_sections<'a>(
    bytes: &'a [u8],
    headers: &[u8],
    names: &'a [u8],
    offset: u64,
) -> Result<Vec<Section<'a>>, Diagnostic> {
    let mut sections = Vec::new();
    let mut seen = BTreeSet::new();
    let mut allocated = 0u64;
    let mut ranges = vec![(0, 64), (offset, offset + headers.len() as u64)];
    for (ordinal, header) in headers.chunks_exact(64).enumerate() {
        if ordinal == 0 {
            sections.push(Section { name: "", size: 0, data: &[] });
            continue;
        }
        let name = string(names, number(header, 0, 4)?)?;
        let (kind, flags) = match name {
            ".text" => (1, 6),
            ".data" => (1, 3),
            ".bss" => (8, 3),
            ".rodata" => (1, 2),
            ".rela.text" => (4, 0x40),
            ".note.GNU-stack" => (1, 0),
            ".symtab" => (2, 0),
            ".strtab" | ".shstrtab" => (3, 0),
            _ => return Err(error()),
        };
        let start = number(header, 24, 8)?;
        let size = number(header, 32, 8)?;
        let alignment = number(header, 48, 8)?;
        if !seen.insert(name)
            || number(header, 4, 4)? != kind
            || number(header, 8, 8)? != flags
            || number(header, 16, 8)? != 0
            || alignment == 0
            || !alignment.is_power_of_two()
            || alignment > 16
            || start % alignment != 0
        {
            return Err(error());
        }
        if flags & 2 != 0 {
            allocated = allocated.checked_add(size).ok_or_else(limit)?;
            if allocated > zryna_backend_native::MAX_NATIVE_OBJECT_BYTES as u64 {
                return Err(limit());
            }
        }
        let data = if kind == 8 {
            if start > bytes.len() as u64 {
                return Err(error());
            }
            &[][..]
        } else {
            let data = range(bytes, start, size)?;
            if size != 0 {
                ranges.push((start, start.checked_add(size).ok_or_else(error)?));
            }
            data
        };
        if kind == 3 && (data.first() != Some(&0) || data.last() != Some(&0)) {
            return Err(error());
        }
        if !matches!(kind, 2 | 4)
            && (number(header, 40, 4)? != 0
                || number(header, 44, 4)? != 0
                || number(header, 56, 8)? != 0)
        {
            return Err(error());
        }
        sections.push(Section { name, size, data });
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(error());
    }
    Ok(sections)
}
