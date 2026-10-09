//! Closed ELF audit for the M1 scalar object boundary.

use crate::MAX_NATIVE_OBJECT_BYTES;
use object::{
    BinaryFormat, Endianness, Object, ObjectKind, ObjectSection, ObjectSymbol, SectionFlags,
    SectionKind,
};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::{VerifiedMirFunction, VerifiedMirModule};

const EMPTY_OBJECT_SECTIONS: [(&str, SectionKind, u64); 4] = [
    (".note.GNU-stack", SectionKind::Other, 0),
    (".symtab", SectionKind::Metadata, 0),
    (".strtab", SectionKind::Metadata, 0),
    (".shstrtab", SectionKind::Metadata, 0),
];
const FUNCTION_OBJECT_SECTIONS: [(&str, SectionKind, u64); 5] = [
    (".text", SectionKind::Text, 6),
    (".note.GNU-stack", SectionKind::Other, 0),
    (".symtab", SectionKind::Metadata, 0),
    (".strtab", SectionKind::Metadata, 0),
    (".shstrtab", SectionKind::Metadata, 0),
];

pub(super) fn audit_object(bytes: &[u8], module: &VerifiedMirModule) -> Result<(), Diagnostic> {
    if bytes.len() > MAX_NATIVE_OBJECT_BYTES {
        return Err(object_audit_error());
    }
    let file = object::File::parse(bytes).map_err(|_| object_audit_error())?;
    if file.format() != BinaryFormat::Elf
        || file.architecture() != object::Architecture::X86_64
        || file.endianness() != Endianness::Little
        || file.kind() != ObjectKind::Relocatable
        || !file.is_64()
    {
        return Err(object_audit_error());
    }
    let expected_sections = if module.functions().len() == 0 {
        &EMPTY_OBJECT_SECTIONS[..]
    } else {
        &FUNCTION_OBJECT_SECTIONS[..]
    };
    let sections = file.sections().collect::<Vec<_>>();
    if sections.len() != expected_sections.len() {
        return Err(object_audit_error());
    }
    for (section, (expected_name, expected_kind, expected_flags)) in
        sections.into_iter().zip(expected_sections.iter().copied())
    {
        let SectionFlags::Elf { sh_flags } = section.flags() else {
            return Err(object_audit_error());
        };
        if section.name().map_err(|_| object_audit_error())? != expected_name
            || section.kind() != expected_kind
            || sh_flags != expected_flags
            || section.relocations().next().is_some()
        {
            return Err(object_audit_error());
        }
    }
    let expected = module.functions().map(VerifiedMirFunction::symbol).collect::<Vec<_>>();
    let mut observed = Vec::new();
    for symbol in file.symbols() {
        if symbol.is_undefined() {
            return Err(object_audit_error());
        }
        if symbol.is_global() {
            if symbol.kind() != object::SymbolKind::Text {
                return Err(object_audit_error());
            }
            let name = symbol.name().map_err(|_| object_audit_error())?;
            if symbol.size() == 0 {
                return Err(object_audit_error());
            }
            observed.push(name);
        }
    }
    if observed != expected {
        return Err(object_audit_error());
    }
    Ok(())
}

fn object_audit_error() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N3003",
        None,
        "native object failed the closed Linux x86-64 ELF audit",
        "report this compiler failure with the smallest reproducible source",
    )
}
