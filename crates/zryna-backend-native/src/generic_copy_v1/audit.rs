//! Independent closed ELF inventory, scalar symbols and direct-call relocation audit.
use crate::MAX_NATIVE_OBJECT_BYTES;
use object::{
    BinaryFormat, Endianness, Object, ObjectKind, ObjectSection, ObjectSymbol, RelocationEncoding,
    RelocationFlags, RelocationKind, RelocationTarget, SectionFlags, SectionKind, SymbolFlags,
    SymbolKind, SymbolScope, SymbolSection,
};
use std::collections::BTreeMap;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::generic_copy_v1::VerifiedProgram;

pub(super) fn check(bytes: &[u8], program: &VerifiedProgram<'_, '_>) -> Result<(), Diagnostic> {
    if bytes.len() > MAX_NATIVE_OBJECT_BYTES {
        return Err(object_audit_error());
    }
    let file = object::File::parse(bytes).map_err(|_| object_audit_error())?;
    if file.format() != BinaryFormat::Elf
        || file.architecture() != object::Architecture::X86_64
        || file.endianness() != Endianness::Little
        || file.kind() != ObjectKind::Relocatable
        || !file.is_64()
        || file.dynamic_symbols().next().is_some()
        || file.dynamic_relocations().is_some_and(|mut relocations| relocations.next().is_some())
    {
        return Err(object_audit_error());
    }

    let functions = program.functions();
    let mut expected_relocations = Vec::new();
    for (index, function) in functions.iter().enumerate() {
        let f = &program.program().functions()[index];
        for block in function.block_order() {
            for instruction in &f.blocks[*block].instructions {
                if let Some(callee) = super::body::target(program, &instruction.operation)? {
                    expected_relocations.push((function.symbol(), functions[callee].symbol()));
                }
            }
        }
    }
    for (export, index) in
        program.program().scalar_abi().exports().zip(program.program().export_functions())
    {
        expected_relocations
            .push((export.native_linux_x86_64_symbol().as_str(), functions[*index].symbol()));
    }

    let expected_sections: Vec<(&str, SectionKind, u64)> = if functions.is_empty() {
        vec![
            (".note.GNU-stack", SectionKind::Other, 0),
            (".symtab", SectionKind::Metadata, 0),
            (".strtab", SectionKind::Metadata, 0),
            (".shstrtab", SectionKind::Metadata, 0),
        ]
    } else if expected_relocations.is_empty() {
        vec![
            (".text", SectionKind::Text, 6),
            (".note.GNU-stack", SectionKind::Other, 0),
            (".symtab", SectionKind::Metadata, 0),
            (".strtab", SectionKind::Metadata, 0),
            (".shstrtab", SectionKind::Metadata, 0),
        ]
    } else {
        vec![
            (".text", SectionKind::Text, 6),
            (".rela.text", SectionKind::Metadata, 64),
            (".note.GNU-stack", SectionKind::Other, 0),
            (".symtab", SectionKind::Metadata, 0),
            (".strtab", SectionKind::Metadata, 0),
            (".shstrtab", SectionKind::Metadata, 0),
        ]
    };
    let sections = file.sections().collect::<Vec<_>>();
    if sections.len() != expected_sections.len() {
        return Err(object_audit_error());
    }
    for (section, (expected_name, expected_kind, expected_flags)) in
        sections.iter().zip(&expected_sections)
    {
        let SectionFlags::Elf { sh_flags } = section.flags() else {
            return Err(object_audit_error());
        };
        if section.name().map_err(|_| object_audit_error())? != *expected_name
            || section.kind() != *expected_kind
            || sh_flags != *expected_flags
            || (*expected_name != ".text" && section.relocations().next().is_some())
        {
            return Err(object_audit_error());
        }
    }

    let text = sections.iter().find(|section| section.name().ok() == Some(".text"));
    if functions.is_empty() && text.is_some() {
        return Err(object_audit_error());
    }
    let text_index = text.map(ObjectSection::index);
    let text_size = text.map_or(0, ObjectSection::size);

    let symbol_by_name = symbols(&file, program, text_index, text_size)?;
    relocations(text, &expected_relocations, &symbol_by_name)
}

type Symbols<'a> = BTreeMap<&'a str, (u64, u64, object::SymbolIndex)>;
fn symbols<'a>(
    file: &object::File<'_>,
    program: &'a VerifiedProgram<'_, '_>,
    text_index: Option<object::SectionIndex>,
    text_size: u64,
) -> Result<Symbols<'a>, Diagnostic> {
    let functions = program.functions();
    let mut expected_symbols =
        Vec::with_capacity(functions.len() + program.program().scalar_abi().exports().len() + 1);
    expected_symbols.push(("zryna-gcopy-v1", SymbolKind::File, SymbolScope::Compilation, false));
    expected_symbols.extend(
        functions
            .iter()
            .map(|function| (function.symbol(), SymbolKind::Text, SymbolScope::Compilation, true)),
    );
    expected_symbols.extend(program.program().scalar_abi().exports().map(|export| {
        (export.native_linux_x86_64_symbol().as_str(), SymbolKind::Text, SymbolScope::Dynamic, true)
    }));
    let symbols = file.symbols().collect::<Vec<_>>();
    if symbols.len() != expected_symbols.len() {
        return Err(object_audit_error());
    }
    let mut symbol_by_name = BTreeMap::new();
    let mut previous_end = 0_u64;
    for (symbol, (expected_name, expected_kind, expected_scope, is_text)) in
        symbols.iter().zip(expected_symbols)
    {
        let expected_info = if expected_kind == SymbolKind::File {
            object::elf::STT_FILE
        } else if expected_scope == SymbolScope::Dynamic {
            (object::elf::STB_GLOBAL << 4) | object::elf::STT_FUNC
        } else {
            object::elf::STT_FUNC
        };
        if symbol.name().map_err(|_| object_audit_error())? != expected_name
            || symbol.kind() != expected_kind
            || symbol.scope() != expected_scope
            || symbol.is_undefined()
            || symbol.is_weak()
            || (symbol.is_global() != (expected_scope == SymbolScope::Dynamic))
            || symbol.flags()
                != (SymbolFlags::Elf { st_info: expected_info, st_other: object::elf::STV_DEFAULT })
        {
            return Err(object_audit_error());
        }
        if is_text {
            let end = symbol.address().checked_add(symbol.size()).ok_or_else(object_audit_error)?;
            if symbol.size() == 0
                || symbol.address() < previous_end
                || end > text_size
                || symbol.section()
                    != text_index.map_or(SymbolSection::None, SymbolSection::Section)
            {
                return Err(object_audit_error());
            }
            previous_end = end;
        } else if symbol.address() != 0
            || symbol.size() != 0
            || symbol.section() != SymbolSection::None
        {
            return Err(object_audit_error());
        }
        if symbol_by_name
            .insert(expected_name, (symbol.address(), symbol.size(), symbol.index()))
            .is_some()
        {
            return Err(object_audit_error());
        }
    }

    Ok(symbol_by_name)
}

fn relocations(
    text: Option<&object::Section<'_, '_>>,
    expected_relocations: &[(&str, &str)],
    symbol_by_name: &Symbols<'_>,
) -> Result<(), Diagnostic> {
    let text_bytes = text
        .map(|section| section.data().map_err(|_| object_audit_error()))
        .transpose()?
        .unwrap_or_default();
    let observed_relocations =
        text.map(|section| section.relocations().collect::<Vec<_>>()).unwrap_or_default();
    if observed_relocations.len() != expected_relocations.len() {
        return Err(object_audit_error());
    }
    let mut previous_offset = None;
    for ((offset, relocation), (caller_name, target_name)) in
        observed_relocations.into_iter().zip(expected_relocations.iter().copied())
    {
        let caller = symbol_by_name.get(caller_name).ok_or_else(object_audit_error)?;
        let target = symbol_by_name.get(target_name).ok_or_else(object_audit_error)?;
        let caller_end = caller.0.checked_add(caller.1).ok_or_else(object_audit_error)?;
        let displacement_end = offset.checked_add(4).ok_or_else(object_audit_error)?;
        let opcode_offset = offset.checked_sub(1).ok_or_else(object_audit_error)?;
        let opcode = text_bytes
            .get(usize::try_from(opcode_offset).map_err(|_| object_audit_error())?)
            .copied();
        if offset <= caller.0
            || displacement_end > caller_end
            || opcode != Some(0xe8)
            || previous_offset.is_some_and(|previous| offset <= previous)
            || relocation.kind() != RelocationKind::PltRelative
            || relocation.encoding() != RelocationEncoding::X86Branch
            || relocation.size() != 32
            || relocation.addend() != -4
            || relocation.has_implicit_addend()
            || relocation.subtractor().is_some()
            || relocation.flags() != (RelocationFlags::Elf { r_type: object::elf::R_X86_64_PLT32 })
            || relocation.target() != RelocationTarget::Symbol(target.2)
        {
            return Err(object_audit_error());
        }
        previous_offset = Some(offset);
    }
    Ok(())
}

fn object_audit_error() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N7103",
        None,
        "generic native object failed its closed Linux x86-64 ELF audit",
        "report the smallest reproducible source",
    )
}
