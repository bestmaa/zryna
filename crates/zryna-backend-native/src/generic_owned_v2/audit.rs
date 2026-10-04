//! Closed ELF inventory plus independently derived caller/callee runtime relocation counts.
use super::{invariant, runtime};
use object::{
    Object, ObjectSection, ObjectSymbol, RelocationEncoding, RelocationFlags, RelocationKind,
    RelocationTarget, SectionFlags, SectionKind, SymbolKind, SymbolScope, SymbolSection,
};
use std::collections::{BTreeMap, BTreeSet};
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::{owned_v2::raw::Operation, raw};
use zryna_native_mir::generic_owned_v2::VerifiedProgram;
#[allow(
    clippy::too_many_lines,
    reason = "The independent object parser exhaustively checks one closed ELF inventory."
)]
pub(super) fn check(bytes: &[u8], p: &VerifiedProgram<'_, '_>) -> Result<(), Diagnostic> {
    if bytes.len() > crate::MAX_NATIVE_OBJECT_BYTES {
        return Err(error());
    }
    let file = object::File::parse(bytes).map_err(|_| error())?;
    if file.format() != object::BinaryFormat::Elf
        || file.architecture() != object::Architecture::X86_64
        || file.endianness() != object::Endianness::Little
        || file.kind() != object::ObjectKind::Relocatable
        || !file.is_64()
        || file.dynamic_symbols().next().is_some()
        || file.dynamic_relocations().is_some_and(|mut r| r.next().is_some())
    {
        return Err(error());
    }
    let sections = file.sections().collect::<Vec<_>>();
    let text = sections.iter().find(|s| s.name().ok() == Some(".text"));
    let mut section_names = BTreeSet::new();
    for s in &sections {
        let name = s.name().map_err(|_| error())?;
        let (kind, flags) = match name {
            ".text" => (SectionKind::Text, 6),
            ".rela.text" => (SectionKind::Metadata, 64),
            ".note.GNU-stack" => (SectionKind::Other, 0),
            ".symtab" | ".strtab" | ".shstrtab" => (SectionKind::Metadata, 0),
            _ => return Err(error()),
        };
        if s.kind() != kind
            || s.flags() != (SectionFlags::Elf { sh_flags: flags })
            || !section_names.insert(name)
            || (name != ".text" && s.relocations().next().is_some())
        {
            return Err(error());
        }
    }
    if ![".note.GNU-stack", ".symtab", ".strtab", ".shstrtab"]
        .iter()
        .all(|n| section_names.contains(n))
    {
        return Err(error());
    }
    let imports = runtime::names(p)?;
    let mut expected = BTreeMap::new();
    for f in p.functions() {
        expected.insert(f.symbol(), SymbolScope::Compilation);
    }
    for export in p.program().scalar_abi().exports() {
        expected.insert(export.native_linux_x86_64_symbol().as_str(), SymbolScope::Dynamic);
    }
    let mut observed = BTreeSet::new();
    let mut observed_imports = BTreeSet::new();
    let mut symbols = BTreeMap::new();
    let mut extents = Vec::new();
    let mut files = 0;
    for symbol in file.symbols() {
        let name = symbol.name().map_err(|_| error())?;
        if symbol.kind() == SymbolKind::File {
            if name != "zryna-gowned-v2"
                || symbol.scope() != SymbolScope::Compilation
                || symbol.address() != 0
                || symbol.size() != 0
                || symbol.section() != SymbolSection::None
                || symbol.flags() != (object::SymbolFlags::Elf { st_info: 4, st_other: 0 })
                || files != 0
            {
                return Err(error());
            }
            files += 1;
            continue;
        }
        if symbol.is_undefined() {
            if !imports.contains(&name)
                || symbol.kind() != SymbolKind::Unknown
                || symbol.scope() != SymbolScope::Unknown
                || symbol.flags() != (object::SymbolFlags::Elf { st_info: 16, st_other: 0 })
                || !symbol.is_global()
                || symbol.is_weak()
                || symbol.address() != 0
                || symbol.size() != 0
                || !observed_imports.insert(name)
            {
                return Err(error());
            }
        } else {
            if expected.get(name) != Some(&symbol.scope())
                || symbol.kind() != SymbolKind::Text
                || symbol.is_weak()
                || symbol.section()
                    != text.map_or(SymbolSection::None, |s| SymbolSection::Section(s.index()))
                || symbol.size() == 0
                || symbol.flags()
                    != (object::SymbolFlags::Elf {
                        st_info: if symbol.scope() == SymbolScope::Compilation { 2 } else { 18 },
                        st_other: 0,
                    })
                || !observed.insert(name)
            {
                return Err(error());
            }
            let end = symbol
                .address()
                .checked_add(symbol.size())
                .filter(|v| *v <= text.map_or(0, ObjectSection::size))
                .ok_or_else(error)?;
            extents.push((symbol.address(), end, name));
        }
        if symbols.insert(symbol.index().0, (name, symbol.address(), symbol.size())).is_some() {
            return Err(error());
        }
    }
    if files != 1
        || observed != expected.keys().copied().collect()
        || observed_imports != imports.iter().copied().collect()
    {
        return Err(error());
    }
    extents.sort_unstable();
    if extents.windows(2).any(|v| v[0].1 > v[1].0) {
        return Err(error());
    }
    let mut expected_edges = edges(p, &imports)?;
    let mut previous = None;
    if let Some(text) = text {
        let code = text.data().map_err(|_| error())?;
        for (offset, r) in text.relocations() {
            let caller = extents
                .iter()
                .find(|(begin, end, _)| {
                    offset > *begin && offset.checked_add(4).is_some_and(|v| v <= *end)
                })
                .map(|v| v.2)
                .ok_or_else(error)?;
            let RelocationTarget::Symbol(target) = r.target() else {
                return Err(error());
            };
            let target = symbols.get(&target.0).ok_or_else(error)?.0;
            if code.get(
                usize::try_from(offset.checked_sub(1).ok_or_else(error)?).map_err(|_| error())?,
            ) != Some(&0xe8)
                || previous.is_some_and(|v| v >= offset)
                || r.kind() != RelocationKind::PltRelative
                || r.encoding() != RelocationEncoding::X86Branch
                || r.size() != 32
                || r.addend() != -4
                || r.has_implicit_addend()
                || r.subtractor().is_some()
                || r.flags() != (RelocationFlags::Elf { r_type: object::elf::R_X86_64_PLT32 })
            {
                return Err(error());
            }
            let count = expected_edges.get_mut(&(caller, target)).ok_or_else(error)?;
            *count = count.checked_sub(1).ok_or_else(error)?;
            previous = Some(offset);
        }
    }
    if expected_edges.values().any(|n| *n != 0) {
        return Err(error());
    }
    Ok(())
}
fn value_type(f: &raw::Function, id: u32) -> Result<raw::Type, Diagnostic> {
    f.blocks
        .iter()
        .flat_map(|b| b.parameters.iter().chain(b.instructions.iter().map(|i| &i.result)))
        .find(|d| d.id == id)
        .map(|d| d.ty)
        .ok_or_else(invariant)
}
fn release_calls(p: &VerifiedProgram<'_, '_>, ty: raw::Type) -> Result<usize, Diagnostic> {
    let raw::Type::Stored(i) = ty else {
        return Err(invariant());
    };
    let t = p.stored_type(i).ok_or_else(invariant)?;
    if t.drop_kind() == 0 {
        return Ok(0);
    }
    match t.key()[0] {
        2 => Ok(1),
        0x14 | 0x15 => t.variants().try_fold(0usize, |sum, (_, payload)| {
            sum.checked_add(
                payload.map_or(Ok(0), |v| release_calls(p, raw::Type::Stored(v.index())))?,
            )
            .ok_or_else(error)
        }),
        _ => Err(invariant()),
    }
}
fn edges<'a>(
    p: &'a VerifiedProgram<'_, '_>,
    imports: &[&'a str],
) -> Result<BTreeMap<(&'a str, &'a str), usize>, Diagnostic> {
    let mut edges = BTreeMap::new();
    let mut add = |from, to, n| {
        *edges.entry((from, to)).or_insert(0) += n;
    };
    for (index, plan) in p.functions().iter().enumerate() {
        let f = &p.program().functions()[index];
        for block in &f.blocks {
            for i in &block.instructions {
                if let Some(ext) = p.program().extension(index, i.result.id) {
                    match ext {
                        Operation::StringLiteral(_) => add(plan.symbol(), imports[0], 1),
                        Operation::CloneString(_) => add(plan.symbol(), imports[1], 1),
                        Operation::Drop(id) => {
                            add(plan.symbol(), imports[2], release_calls(p, value_type(f, *id)?)?);
                        }
                        _ => {}
                    }
                } else if let Some(target) = super::body::target(p, &i.operation)? {
                    add(plan.symbol(), p.functions()[target].symbol(), 1);
                }
            }
        }
        for step in &p.program().plan(index).ok_or_else(invariant)?.steps {
            for id in &step.cleanup {
                add(plan.symbol(), imports[2], release_calls(p, value_type(f, *id)?)?);
            }
        }
    }
    for (export, target) in p.program().scalar_abi().exports().zip(p.program().export_functions()) {
        add(export.native_linux_x86_64_symbol().as_str(), p.functions()[*target].symbol(), 1);
    }
    Ok(edges)
}
fn error() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N7103",
        None,
        "generic owned ELF failed its exact runtime/call inventory audit",
        "retain the source-bound owned program and pinned runtime declarations",
    )
}
