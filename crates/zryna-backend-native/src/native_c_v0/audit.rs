//! Independent closed structural ELF audit for the exact total-scalar export inventory.

use super::audit_error;
use object::{
    BinaryFormat, Endianness, Object, ObjectKind, ObjectSection, ObjectSymbol, SectionFlags,
    SectionKind, SymbolFlags, SymbolKind, SymbolSection,
};
use std::collections::BTreeSet;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::VerifiedMirProgram;

pub(super) fn check(bytes: &[u8], program: &VerifiedMirProgram) -> Result<(), Diagnostic> {
    if bytes.len() > crate::MAX_NATIVE_OBJECT_BYTES {
        return Err(audit_error());
    }
    let file = object::File::parse(bytes).map_err(|_| audit_error())?;
    if file.format() != BinaryFormat::Elf
        || file.architecture() != object::Architecture::X86_64
        || file.endianness() != Endianness::Little
        || file.kind() != ObjectKind::Relocatable
        || !file.is_64()
    {
        return Err(audit_error());
    }
    let operations = program.operations().collect::<Vec<_>>();
    let expected = program
        .functions()
        .filter_map(zryna_native_mir::native_c_v0::VerifiedFunction::export)
        .map(|index| operations[index].declaration().symbol.as_str())
        .collect::<BTreeSet<_>>();
    let mut observed = BTreeSet::new();
    let mut text = None;
    let mut sections = Vec::new();
    for section in file.sections() {
        let name = section.name().map_err(|_| audit_error())?;
        let (kind, flags) = match name {
            ".text" => {
                text = Some((section.index(), section.size()));
                (SectionKind::Text, 6)
            }
            ".note.GNU-stack" => (SectionKind::Other, 0),
            ".symtab" | ".strtab" | ".shstrtab" => (SectionKind::Metadata, 0),
            _ => return Err(audit_error()),
        };
        if section.data().map_err(|_| audit_error())?.len()
            != usize::try_from(section.size()).map_err(|_| audit_error())?
            || section.kind() != kind
            || section.flags() != (SectionFlags::Elf { sh_flags: flags })
            || section.relocations().next().is_some()
        {
            return Err(audit_error());
        }
        sections.push(name);
    }
    let required = if expected.is_empty() {
        &[".note.GNU-stack", ".symtab", ".strtab", ".shstrtab"][..]
    } else {
        &[".text", ".note.GNU-stack", ".symtab", ".strtab", ".shstrtab"][..]
    };
    if sections != required {
        return Err(audit_error());
    }
    let mut ranges = Vec::new();
    let mut file_symbols = 0;
    for symbol in file.symbols() {
        if symbol.is_undefined() {
            return Err(audit_error());
        }
        if symbol.is_global() {
            let name = symbol.name().map_err(|_| audit_error())?;
            let Some((text_index, text_size)) = text else { return Err(audit_error()) };
            let end = symbol.address().checked_add(symbol.size()).ok_or_else(audit_error)?;
            if symbol.kind() != SymbolKind::Text
                || symbol.section() != SymbolSection::Section(text_index)
                || symbol.size() == 0
                || end > text_size
                || !expected.contains(name)
                || !observed.insert(name)
            {
                return Err(audit_error());
            }
            let SymbolFlags::Elf { st_info, st_other } = symbol.flags() else {
                return Err(audit_error());
            };
            if st_info != 0x12 || st_other != 0 {
                return Err(audit_error());
            }
            ranges.push((symbol.address(), end));
        } else {
            if symbol.kind() != SymbolKind::File
                || symbol.name().map_err(|_| audit_error())? != "zryna-native-c-exports-v0"
                || symbol.section() != SymbolSection::None
                || symbol.address() != 0
                || symbol.size() != 0
                || symbol.flags() != (SymbolFlags::Elf { st_info: 4, st_other: 0 })
            {
                return Err(audit_error());
            }
            file_symbols += 1;
        }
    }
    ranges.sort_unstable();
    if file_symbols != 1
        || observed != expected
        || ranges.windows(2).any(|pair| pair[0].1 > pair[1].0)
    {
        return Err(audit_error());
    }
    Ok(())
}
