//! Independent closed ELF inventory and call relocation admission from the original machine seal.

use super::{super::audit_error, admit, ledger, release_emit, storage};
use object::{
    BinaryFormat, Endianness, Object, ObjectKind, ObjectSection, ObjectSymbol, RelocationFlags,
    RelocationTarget, SectionFlags, SectionKind, SymbolFlags, SymbolKind, SymbolSection,
};
use std::collections::BTreeSet;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::VerifiedMirProgram;

pub(super) fn check(
    bytes: &[u8],
    program: &VerifiedMirProgram,
    selected: &[usize],
) -> Result<(), Diagnostic> {
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
    let mut expected = program
        .functions()
        .enumerate()
        .filter(|(ordinal, _)| selected.contains(ordinal))
        .map(|(_, function)| function.entry().symbol.as_str())
        .collect::<BTreeSet<_>>();
    expected.insert(program.dispatcher().symbol.as_str());
    let mut helper_names = ledger::SYMBOLS.into_iter().map(str::to_owned).collect::<BTreeSet<_>>();
    helper_names
        .extend(release_emit::operations(program, selected).into_iter().map(release_emit::symbol));
    helper_names.extend(storage::helper_names(program, selected));
    let runtime_imports = storage::runtime_imports(program, selected);
    let required = admit::imports(program, selected);
    let operations = program.operations().collect::<Vec<_>>();
    let imports = required
        .iter()
        .map(|index| operations[*index].declaration().symbol.as_str())
        .chain(runtime_imports.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    let (text_index, text_size) = sections(&file)?;
    let ranges = symbols(&file, text_index, text_size, &expected, &helper_names, &imports)?;
    let text = file.section_by_index(text_index).map_err(|_| audit_error())?;
    let mut relocations = BTreeSet::new();
    for (offset, relocation) in text.relocations() {
        let RelocationTarget::Symbol(index) = relocation.target() else {
            return Err(audit_error());
        };
        let symbol = file.symbol_by_index(index).map_err(|_| audit_error())?;
        let name = symbol.name().map_err(|_| audit_error())?;
        if !imports.contains(name) && !expected.contains(name) && !helper_names.contains(name) {
            return Err(audit_error());
        }
        let (width, addend, kind) = if imports.contains(name) {
            (8, 0, object::elf::R_X86_64_64)
        } else {
            (4, -4, object::elf::R_X86_64_PLT32)
        };
        let end = offset.checked_add(width).ok_or_else(audit_error)?;
        if end > text_size
            || !relocations.insert((offset, end))
            || !ranges.iter().any(|(start, limit)| *start <= offset && end <= *limit)
            || relocation.addend() != addend
            || relocation.flags() != (RelocationFlags::Elf { r_type: kind })
        {
            return Err(audit_error());
        }
    }
    let fields = relocations.into_iter().collect::<Vec<_>>();
    if fields.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(audit_error());
    }
    Ok(())
}

fn sections(file: &object::File<'_>) -> Result<(object::SectionIndex, u64), Diagnostic> {
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
            ".rela.text" => (SectionKind::Metadata, 0x40),
            ".symtab" | ".strtab" | ".shstrtab" => (SectionKind::Metadata, 0),
            _ => return Err(audit_error()),
        };
        if section.kind() != kind
            || section.flags() != (SectionFlags::Elf { sh_flags: flags })
            || section.data().map_err(|_| audit_error())?.len()
                != usize::try_from(section.size()).map_err(|_| audit_error())?
        {
            return Err(audit_error());
        }
        if name != ".text" && section.relocations().next().is_some() {
            return Err(audit_error());
        }
        sections.push(name);
    }
    if sections != [".text", ".rela.text", ".note.GNU-stack", ".symtab", ".strtab", ".shstrtab"] {
        return Err(audit_error());
    }
    text.ok_or_else(audit_error)
}

fn symbols(
    file: &object::File<'_>,
    text_index: object::SectionIndex,
    text_size: u64,
    expected: &BTreeSet<&str>,
    helper_names: &BTreeSet<String>,
    imports: &BTreeSet<&str>,
) -> Result<Vec<(u64, u64)>, Diagnostic> {
    let mut defined = BTreeSet::new();
    let mut helpers = BTreeSet::new();
    let mut undefined = BTreeSet::new();
    let mut ranges = Vec::new();
    let mut file_symbols = 0;
    for symbol in file.symbols() {
        let name = symbol.name().map_err(|_| audit_error())?;
        if symbol.is_undefined() {
            if !imports.contains(name)
                || !undefined.insert(name)
                || symbol.address() != 0
                || symbol.size() != 0
                || !symbol.is_global()
                || symbol.kind() != SymbolKind::Unknown
                || symbol.flags() != (SymbolFlags::Elf { st_info: 0x10, st_other: 0 })
            {
                return Err(audit_error());
            }
        } else if symbol.kind() == SymbolKind::Text {
            let end = symbol.address().checked_add(symbol.size()).ok_or_else(audit_error)?;
            if symbol.section() != SymbolSection::Section(text_index)
                || symbol.size() == 0
                || end > text_size
            {
                return Err(audit_error());
            }
            let accepted = if symbol.is_global() {
                expected.contains(name)
                    && defined.insert(name)
                    && symbol.flags() == (SymbolFlags::Elf { st_info: 0x12, st_other: 2 })
            } else {
                helper_names.contains(name)
                    && helpers.insert(name.to_owned())
                    && symbol.flags() == (SymbolFlags::Elf { st_info: 2, st_other: 0 })
            };
            if !accepted {
                return Err(audit_error());
            }
            ranges.push((symbol.address(), end));
        } else {
            if symbol.kind() != SymbolKind::File
                || name != "zryna-native-c-handles-v0"
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
    if &defined != expected
        || &helpers != helper_names
        || &undefined != imports
        || file_symbols != 1
        || ranges.windows(2).any(|pair| pair[0].1 > pair[1].0)
    {
        return Err(audit_error());
    }
    Ok(ranges)
}
