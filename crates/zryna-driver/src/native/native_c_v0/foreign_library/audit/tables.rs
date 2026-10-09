//! Raw ELF table validation before iterators that can omit malformed relocation arrays.

use super::{Diagnostic, MAX_RELOCATIONS, MAX_SECTIONS, MAX_SYMBOLS, audit_error, limit_error};
use std::collections::BTreeMap;

fn number(bytes: &[u8], offset: usize, width: usize) -> Result<u64, Diagnostic> {
    let source = bytes
        .get(offset..offset.checked_add(width).ok_or_else(audit_error)?)
        .ok_or_else(audit_error)?;
    let mut value = [0; 8];
    value[..width].copy_from_slice(source);
    Ok(u64::from_le_bytes(value))
}
fn index(value: u64) -> Result<usize, Diagnostic> {
    usize::try_from(value).map_err(|_| audit_error())
}
fn data(bytes: &[u8], offset: u64, length: u64) -> Result<&[u8], Diagnostic> {
    bytes
        .get(index(offset)?..index(offset.checked_add(length).ok_or_else(audit_error)?)?)
        .ok_or_else(audit_error)
}

pub(super) fn check(bytes: &[u8]) -> Result<(), Diagnostic> {
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
        return Err(audit_error());
    }
    let count = index(number(bytes, 60, 2)?)?;
    if count > MAX_SECTIONS + 1 {
        return Err(limit_error());
    }
    let offset = number(bytes, 40, 8)?;
    if count < 2 || offset < 64 || offset % 8 != 0 {
        return Err(audit_error());
    }
    let table = data(bytes, offset, (count * 64) as u64)?;
    if table[..64].iter().any(|byte| *byte != 0) {
        return Err(audit_error());
    }
    let names_index = index(number(bytes, 62, 2)?)?;
    let names = section_inventory(bytes, table, offset, names_index)?;
    check_records(bytes, table, &names)
}

fn section_inventory<'a>(
    bytes: &'a [u8],
    table: &[u8],
    offset: u64,
    names_index: usize,
) -> Result<BTreeMap<&'a str, usize>, Diagnostic> {
    let count = table.len() / 64;
    let names_header =
        table.get(names_index * 64..(names_index + 1) * 64).ok_or_else(audit_error)?;
    let strings = data(bytes, number(names_header, 24, 8)?, number(names_header, 32, 8)?)?;
    let mut names = BTreeMap::new();
    let mut ranges = vec![(0, 64), (offset, offset + table.len() as u64)];
    for ordinal in 1..count {
        let header = &table[ordinal * 64..(ordinal + 1) * 64];
        let start = index(number(header, 0, 4)?)?;
        let name = strings
            .get(start..)
            .and_then(|s| s.split(|byte| *byte == 0).next())
            .and_then(|s| std::str::from_utf8(s).ok())
            .ok_or_else(audit_error)?;
        if !strings.get(start..).is_some_and(|s| s.contains(&0))
            || names.insert(name, ordinal).is_some()
        {
            return Err(audit_error());
        }
        let kind = number(header, 4, 4)?;
        let expected = match name {
            ".text" | ".data" | ".rodata" | ".note.GNU-stack" => 1,
            ".bss" => 8,
            ".rela.text" => 4,
            ".symtab" => 2,
            ".strtab" | ".shstrtab" => 3,
            _ => return Err(audit_error()),
        };
        if kind != expected {
            return Err(audit_error());
        }
        let start = number(header, 24, 8)?;
        let size = number(header, 32, 8)?;
        let align = number(header, 48, 8)?;
        if align == 0 || !align.is_power_of_two() || align > 16 || start % align != 0 {
            return Err(audit_error());
        }
        if kind == 8 {
            if start > bytes.len() as u64 {
                return Err(audit_error());
            }
        } else {
            let section = data(bytes, start, size)?;
            if size > 0 {
                ranges.push((start, start + size));
            }
            if kind == 3 && (section.first() != Some(&0) || section.last() != Some(&0)) {
                return Err(audit_error());
            }
        }
        if !matches!(kind, 2 | 4)
            && (number(header, 40, 4)? != 0
                || number(header, 44, 4)? != 0
                || number(header, 56, 8)? != 0)
        {
            return Err(audit_error());
        }
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0)
        || names.get(".shstrtab") != Some(&names_index)
    {
        return Err(audit_error());
    }
    Ok(names)
}

fn check_records(
    bytes: &[u8],
    table: &[u8],
    names: &BTreeMap<&str, usize>,
) -> Result<(), Diagnostic> {
    let symbols_index = *names.get(".symtab").ok_or_else(audit_error)?;
    let sym = &table[symbols_index * 64..(symbols_index + 1) * 64];
    let symbol_bytes = data(bytes, number(sym, 24, 8)?, number(sym, 32, 8)?)?;
    let symbol_count = symbol_bytes.len() / 24;
    if symbol_count > MAX_SYMBOLS + 1 {
        return Err(limit_error());
    }
    let first_global = index(number(sym, 44, 4)?)?;
    if number(sym, 56, 8)? != 24
        || symbol_bytes.len() % 24 != 0
        || symbol_count == 0
        || symbol_bytes[..24].iter().any(|byte| *byte != 0)
        || first_global == 0
        || first_global > symbol_count
        || names.get(".strtab") != Some(&index(number(sym, 40, 4)?)?)
    {
        return Err(audit_error());
    }
    for (ordinal, symbol) in symbol_bytes.chunks_exact(24).enumerate().skip(1) {
        if (symbol[4] >> 4 == 0) != (ordinal < first_global) {
            return Err(audit_error());
        }
    }
    if let Some(relocations_index) = names.get(".rela.text") {
        let rel = &table[relocations_index * 64..(relocations_index + 1) * 64];
        let relocations = data(bytes, number(rel, 24, 8)?, number(rel, 32, 8)?)?;
        if relocations.len() / 24 > MAX_RELOCATIONS {
            return Err(limit_error());
        }
        if number(rel, 56, 8)? != 24
            || relocations.len() % 24 != 0
            || index(number(rel, 40, 4)?)? != symbols_index
            || names.get(".text") != Some(&index(number(rel, 44, 4)?)?)
        {
            return Err(audit_error());
        }
        for relocation in relocations.chunks_exact(24) {
            let target = index(number(relocation, 8, 8)? >> 32)?;
            if target == 0 || target >= symbol_count {
                return Err(audit_error());
            }
        }
    }
    Ok(())
}
