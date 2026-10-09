//! Explicit bounded non-PIC relocation fields and targets; no deferred linker interpretation.

use super::{
    BTreeSet, Diagnostic, MAX_RELOCATIONS, Object, ObjectSection, ObjectSymbol, RelocationFlags,
    RelocationTarget, SymbolKind, SymbolSection, audit_error, limit_error,
};

pub(super) fn check(
    file: &object::File<'_>,
    definitions: &BTreeSet<String>,
    dependencies: &BTreeSet<String>,
) -> Result<(), Diagnostic> {
    let text = file.section_by_name(".text").ok_or_else(audit_error)?;
    let mut fields = Vec::new();
    let mut count = 0;
    for (offset, relocation) in text.relocations() {
        count += 1;
        if count > MAX_RELOCATIONS {
            return Err(limit_error());
        }
        let RelocationFlags::Elf { r_type } = relocation.flags() else { return Err(audit_error()) };
        let width = match r_type {
            object::elf::R_X86_64_64 => 8,
            object::elf::R_X86_64_32
            | object::elf::R_X86_64_32S
            | object::elf::R_X86_64_PC32
            | object::elf::R_X86_64_PLT32 => 4,
            _ => return Err(audit_error()),
        };
        let end = offset.checked_add(width).ok_or_else(audit_error)?;
        if end > text.size() || relocation.has_implicit_addend() {
            return Err(audit_error());
        }
        let RelocationTarget::Symbol(index) = relocation.target() else {
            return Err(audit_error());
        };
        let target = file.symbol_by_index(index).map_err(|_| audit_error())?;
        let name = target.name().map_err(|_| audit_error())?;
        if target.is_undefined() {
            if !dependencies.contains(name)
                || r_type != object::elf::R_X86_64_PLT32
                || relocation.addend() != -4
            {
                return Err(audit_error());
            }
        } else if target.kind() == SymbolKind::Text {
            if target.is_global() && !definitions.contains(name)
                || !matches!(r_type, object::elf::R_X86_64_PC32 | object::elf::R_X86_64_PLT32)
                || relocation.addend() != -4
            {
                return Err(audit_error());
            }
        } else if matches!(target.kind(), SymbolKind::Section | SymbolKind::Data) {
            let SymbolSection::Section(index) = target.section() else { return Err(audit_error()) };
            let section = file.section_by_index(index).map_err(|_| audit_error())?;
            if !matches!(section.name().map_err(|_| audit_error())?, ".data" | ".bss" | ".rodata")
                || r_type == object::elf::R_X86_64_PLT32
            {
                return Err(audit_error());
            }
            let addend = relocation
                .addend()
                .checked_add(if r_type == object::elf::R_X86_64_PC32 { 4 } else { 0 })
                .ok_or_else(audit_error)?;
            let addend = u64::try_from(addend).map_err(|_| audit_error())?;
            if target.address().checked_add(addend).is_none_or(|offset| offset >= section.size()) {
                return Err(audit_error());
            }
        } else {
            return Err(audit_error());
        }
        fields.push((offset, end));
    }
    fields.sort_unstable();
    if fields.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(audit_error());
    }
    Ok(())
}
