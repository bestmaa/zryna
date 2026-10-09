//! Hostile derivatives of actual compiler output; expected ABI never comes from these bytes.

use object::{Object as _, ObjectSection as _, ObjectSymbol as _};

pub(super) fn word(bytes: &[u8], offset: usize, width: usize) -> u64 {
    bytes[offset..offset + width]
        .iter()
        .enumerate()
        .fold(0, |v, (i, b)| v | (u64::from(*b) << (i * 8)))
}

pub(super) fn put(bytes: &mut [u8], offset: usize, width: usize, value: u64) {
    bytes[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
}

pub(super) fn section(bytes: &[u8], name: &str) -> usize {
    let object = object::File::parse(bytes).expect("original ELF");
    let i = object.section_by_name(name).expect("original section").index().0;
    usize::try_from(word(bytes, 40, 8)).expect("header") + i * 64
}

pub(super) fn data(bytes: &[u8], name: &str) -> (usize, usize) {
    let header = section(bytes, name);
    (
        usize::try_from(word(bytes, header + 24, 8)).expect("offset"),
        usize::try_from(word(bytes, header + 32, 8)).expect("size"),
    )
}

pub(super) fn symbol(bytes: &[u8], name: &str) -> usize {
    let object = object::File::parse(bytes).expect("original ELF");
    let i = object.symbols().find(|s| s.name() == Ok(name)).expect("original symbol").index().0;
    data(bytes, ".symtab").0 + i * 24
}

fn relocate_section(bytes: &mut Vec<u8>, header: usize, payload: &[u8]) {
    while !bytes.len().is_multiple_of(8) {
        bytes.push(0);
    }
    let offset = bytes.len() as u64;
    bytes.extend_from_slice(payload);
    put(bytes, header + 24, 8, offset);
    put(bytes, header + 32, 8, payload.len() as u64);
}

pub(super) fn resized_table(original: &[u8], name: &str, size: usize) -> Vec<u8> {
    let mut bytes = original.to_vec();
    let (start, length) = data(original, name);
    let mut payload = original[start..start + length].to_vec();
    payload.resize(size, 0);
    relocate_section(&mut bytes, section(original, name), &payload);
    bytes
}

pub(super) fn append_symbol(original: &[u8], template: usize, name: Option<&str>) -> Vec<u8> {
    let mut bytes = original.to_vec();
    let header = section(original, ".symtab");
    let (start, size) = data(original, ".symtab");
    let mut symbols = original[start..start + size].to_vec();
    let mut row = original[template..template + 24].to_vec();
    if let Some(name) = name {
        let strings_header = section(original, ".strtab");
        let (start, size) = data(original, ".strtab");
        let mut strings = original[start..start + size].to_vec();
        put(&mut row, 0, 4, strings.len() as u64);
        strings.extend_from_slice(name.as_bytes());
        strings.push(0);
        relocate_section(&mut bytes, strings_header, &strings);
    }
    symbols.extend(row);
    relocate_section(&mut bytes, header, &symbols);
    bytes
}

pub(super) fn remove_definition(original: &[u8], name: &str) -> Vec<u8> {
    let mut bytes = original.to_vec();
    let header = section(original, ".symtab");
    let (start, size) = data(original, ".symtab");
    let offset = symbol(original, name);
    let ordinal = (offset - start) / 24;
    let mut symbols = original[start..start + size].to_vec();
    symbols.drain(ordinal * 24..(ordinal + 1) * 24);
    let (relocations, size) = data(original, ".rela.text");
    for i in (relocations..relocations + size).step_by(24) {
        let info = word(original, i + 8, 8);
        assert_ne!(info >> 32, ordinal as u64, "chosen unused runtime definition");
        if info >> 32 > ordinal as u64 {
            put(&mut bytes, i + 8, 8, info - (1u64 << 32));
        }
    }
    relocate_section(&mut bytes, header, &symbols);
    bytes
}

pub(super) fn rename_section(original: &[u8], old: &str, new: &str) -> Vec<u8> {
    let mut bytes = original.to_vec();
    let header = section(original, old);
    let names_header = section(original, ".shstrtab");
    let (start, size) = data(original, ".shstrtab");
    let mut names = original[start..start + size].to_vec();
    put(&mut bytes, header, 4, names.len() as u64);
    names.extend_from_slice(new.as_bytes());
    names.push(0);
    relocate_section(&mut bytes, names_header, &names);
    bytes
}

pub(super) fn rename_symbol(original: &[u8], template: usize, name: &str) -> Vec<u8> {
    let mut bytes = original.to_vec();
    let (start, size) = data(original, ".strtab");
    let mut strings = original[start..start + size].to_vec();
    put(&mut bytes, template, 4, strings.len() as u64);
    strings.extend_from_slice(name.as_bytes());
    strings.push(0);
    relocate_section(&mut bytes, section(original, ".strtab"), &strings);
    bytes
}

pub(super) fn local_definition(original: &[u8], kind: object::SymbolKind) -> usize {
    let object = object::File::parse(original).expect("original ELF");
    let symbol = object
        .symbols()
        .find(|s| s.is_local() && s.kind() == kind)
        .expect("original local definition");
    data(original, ".symtab").0 + symbol.index().0 * 24
}
