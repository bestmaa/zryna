//! Bounded non-PIC relocation fields and validated symbol targets; no deferred interpretation.

use super::{
    Diagnostic, error,
    inventory::Symbol,
    tables::{self, Tables},
};

pub(super) fn check(table: &Tables<'_>, symbols: &[Symbol<'_>]) -> Result<(), Diagnostic> {
    let mut fields = Vec::new();
    for row in table.relocations.chunks_exact(24) {
        let offset = tables::number(row, 0, 8)?;
        let info = tables::number(row, 8, 8)?;
        let ordinal = usize::try_from(info >> 32).map_err(|_| error())?;
        if ordinal == 0 {
            return Err(error());
        }
        let target = symbols.get(ordinal).ok_or_else(error)?;
        let kind = u32::try_from(info & u64::from(u32::MAX)).map_err(|_| error())?;
        let width = match kind {
            object::elf::R_X86_64_64 => 8,
            object::elf::R_X86_64_32
            | object::elf::R_X86_64_32S
            | object::elf::R_X86_64_PC32
            | object::elf::R_X86_64_PLT32 => 4,
            _ => return Err(error()),
        };
        let end = offset.checked_add(width).ok_or_else(error)?;
        if end > table.sections[table.text_index].size {
            return Err(error());
        }
        let addend = i64::from_le_bytes(row[16..24].try_into().map_err(|_| error())?);
        if target.section == 0 {
            if !super::IMPORTS.contains(&target.name)
                || kind != object::elf::R_X86_64_PLT32
                || addend != -4
            {
                return Err(error());
            }
        } else if target.kind == 2 {
            if !matches!(kind, object::elf::R_X86_64_PC32 | object::elf::R_X86_64_PLT32)
                || addend != -4
            {
                return Err(error());
            }
        } else if matches!(target.kind, 1 | 3) {
            let section = table.sections.get(target.section).ok_or_else(error)?;
            if !matches!(section.name, ".data" | ".bss" | ".rodata")
                || kind == object::elf::R_X86_64_PLT32
            {
                return Err(error());
            }
            let addend = addend
                .checked_add(if kind == object::elf::R_X86_64_PC32 { 4 } else { 0 })
                .ok_or_else(error)?;
            let referenced = target
                .value
                .checked_add(u64::try_from(addend).map_err(|_| error())?)
                .ok_or_else(error)?;
            if referenced >= section.size {
                return Err(error());
            }
        } else {
            return Err(error());
        }
        fields.push((offset, end));
    }
    fields.sort_unstable();
    if fields.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(error());
    }
    Ok(())
}
