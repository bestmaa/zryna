//! Closed GCC-style non-PIC ELF inventory. This checks structure, not C body behavior.

use object::{
    BinaryFormat, Endianness, Object, ObjectKind, ObjectSection, ObjectSymbol, RelocationFlags,
    RelocationTarget, SectionFlags, SectionKind, SymbolFlags, SymbolKind, SymbolSection,
};
use std::collections::BTreeSet;
use zryna_diagnostics::Diagnostic;

mod relocations;
mod tables;

pub(super) const MAX_SECTIONS: usize = 16;
pub(super) const MAX_SYMBOLS: usize = 4096;
pub(super) const MAX_RELOCATIONS: usize = 32768;
const MAX_DEPENDENCIES: usize = 64;

pub(super) fn check(
    bytes: &[u8],
    definitions: &BTreeSet<String>,
    supplied: &[&str],
) -> Result<BTreeSet<String>, Diagnostic> {
    if supplied.len() > MAX_DEPENDENCIES {
        return Err(limit_error());
    }
    if supplied.iter().any(|name| !identifier(name) || reserved(name))
        || supplied.windows(2).any(|pair| pair[0] >= pair[1])
        || supplied
            .iter()
            .any(|name| definitions.iter().any(|defined| defined.eq_ignore_ascii_case(name)))
    {
        return Err(audit_error());
    }
    let dependencies = supplied.iter().map(|name| (*name).to_owned()).collect::<BTreeSet<_>>();
    tables::check(bytes)?;
    let file = object::File::parse(bytes).map_err(|_| audit_error())?;
    if file.format() != BinaryFormat::Elf
        || file.architecture() != object::Architecture::X86_64
        || file.endianness() != Endianness::Little
        || file.kind() != ObjectKind::Relocatable
        || !file.is_64()
    {
        return Err(audit_error());
    }
    sections(&file)?;
    symbols(&file, definitions, &dependencies)?;
    relocations::check(&file, definitions, &dependencies)?;
    Ok(dependencies)
}

fn sections(file: &object::File<'_>) -> Result<(), Diagnostic> {
    let mut names = BTreeSet::new();
    let mut ranges = Vec::new();
    let mut allocated = 0u64;
    for section in file.sections() {
        if names.len() >= MAX_SECTIONS {
            return Err(limit_error());
        }
        let name = section.name().map_err(|_| audit_error())?;
        let (kind, flags) = match name {
            ".text" => (SectionKind::Text, 6),
            ".data" => (SectionKind::Data, 3),
            ".bss" => (SectionKind::UninitializedData, 3),
            ".rodata" => (SectionKind::ReadOnlyData, 2),
            ".rela.text" => (SectionKind::Metadata, 0x40),
            ".note.GNU-stack" => (SectionKind::Other, 0),
            ".symtab" | ".strtab" | ".shstrtab" => (SectionKind::Metadata, 0),
            _ => return Err(audit_error()),
        };
        if !names.insert(name)
            || section.kind() != kind
            || section.flags() != (SectionFlags::Elf { sh_flags: flags })
            || section.address() != 0
            || section.align() == 0
            || !section.align().is_power_of_two()
            || section.align() > 16
            || (name != ".text" && section.relocations().next().is_some())
        {
            return Err(audit_error());
        }
        if flags & 2 != 0 {
            allocated = allocated.checked_add(section.size()).ok_or_else(limit_error)?;
            if allocated > zryna_backend_native::MAX_NATIVE_OBJECT_BYTES as u64 {
                return Err(limit_error());
            }
        }
        if name == ".bss" {
            continue;
        }
        if section.data().map_err(|_| audit_error())?.len() as u64 != section.size() {
            return Err(audit_error());
        }
        if let Some((start, size)) = section.file_range() {
            let end = start.checked_add(size).ok_or_else(audit_error)?;
            if size > 0 {
                ranges.push((start, end));
            }
        }
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0)
        || ![".text", ".note.GNU-stack", ".symtab", ".strtab", ".shstrtab"]
            .iter()
            .all(|name| names.contains(name))
    {
        return Err(audit_error());
    }
    Ok(())
}

fn symbols(
    file: &object::File<'_>,
    expected: &BTreeSet<String>,
    dependencies: &BTreeSet<String>,
) -> Result<(), Diagnostic> {
    let mut definitions = BTreeSet::new();
    let mut undefined = BTreeSet::new();
    let mut ranges = Vec::new();
    let mut count = 0;
    for symbol in file.symbols() {
        count += 1;
        if count > MAX_SYMBOLS {
            return Err(limit_error());
        }
        let name = symbol.name().map_err(|_| audit_error())?;
        let SymbolFlags::Elf { st_info, st_other } = symbol.flags() else {
            return Err(audit_error());
        };
        if symbol.is_weak() || symbol.is_common() || st_other & !3 != 0 {
            return Err(audit_error());
        }
        if symbol.is_undefined() {
            if !dependencies.contains(name)
                || !undefined.insert(name.to_owned())
                || symbol.address() != 0
                || symbol.size() != 0
                || st_info != 0x10
                || st_other != 0
            {
                return Err(audit_error());
            }
        } else if symbol.kind() == SymbolKind::Text {
            let SymbolSection::Section(index) = symbol.section() else { return Err(audit_error()) };
            let section = file.section_by_index(index).map_err(|_| audit_error())?;
            let end = symbol.address().checked_add(symbol.size()).ok_or_else(audit_error)?;
            if section.name().map_err(|_| audit_error())? != ".text"
                || symbol.size() == 0
                || end > section.size()
                || !identifier(name)
                || (symbol.is_global()
                    && (!expected.contains(name) || !definitions.insert(name.to_owned())))
                || (!symbol.is_global() && reserved(name))
                || st_info != (if symbol.is_global() { 0x12 } else { 2 })
                || !matches!(st_other, 0 | 2)
            {
                return Err(audit_error());
            }
            ranges.push((symbol.address(), end));
        } else if symbol.is_global()
            || !matches!(symbol.kind(), SymbolKind::File | SymbolKind::Section | SymbolKind::Data)
        {
            return Err(audit_error());
        } else if symbol.kind() == SymbolKind::Data {
            let SymbolSection::Section(index) = symbol.section() else { return Err(audit_error()) };
            let section = file.section_by_index(index).map_err(|_| audit_error())?;
            if !matches!(section.name().map_err(|_| audit_error())?, ".data" | ".bss" | ".rodata")
                || symbol
                    .address()
                    .checked_add(symbol.size())
                    .is_none_or(|end| end > section.size())
                || !identifier(name)
                || reserved(name)
                || st_info != 1
                || st_other != 0
            {
                return Err(audit_error());
            }
        } else if symbol.kind() == SymbolKind::Section {
            let SymbolSection::Section(index) = symbol.section() else { return Err(audit_error()) };
            if !matches!(
                file.section_by_index(index)
                    .map_err(|_| audit_error())?
                    .name()
                    .map_err(|_| audit_error())?,
                ".text" | ".data" | ".bss" | ".rodata"
            ) || symbol.address() != 0
                || symbol.size() != 0
                || st_info != 3
                || st_other != 0
            {
                return Err(audit_error());
            }
        } else if symbol.section() != SymbolSection::None
            || symbol.address() != 0
            || symbol.size() != 0
            || st_info != 4
            || st_other != 0
        {
            return Err(audit_error());
        }
    }
    ranges.sort_unstable();
    if &definitions != expected
        || &undefined != dependencies
        || ranges.windows(2).any(|pair| pair[0].1 > pair[1].0)
    {
        return Err(audit_error());
    }
    Ok(())
}

pub(super) fn identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic() || byte == b'_' || (index > 0 && byte.is_ascii_digit())
        })
}
pub(super) fn reserved(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with("zryna_")
}
pub(super) fn audit_error() -> Diagnostic {
    super::super::super::native_error(
        "ZRYNA-C4104",
        "foreign ELF object violates the closed static library inventory",
        "supply exact strong C functions and reviewed external symbols without dynamic, TLS, constructor or private-runtime definitions",
    )
}
pub(super) fn limit_error() -> Diagnostic {
    super::super::super::native_error(
        "ZRYNA-C4105",
        "foreign ELF capture exceeds its bounded artifact inventory",
        "use one bounded relocatable object within the documented capture limits",
    )
}
