//! Independent ELF field mutations, including fields unrelated to the matching producer.

use super::*;
use object::{Object, ObjectSection};

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("complete ELF field"))
}

#[test]
fn handle_audit_rejects_hostile_sections_symbols_and_call_relocations() {
    let mir = fixture();
    let name = mir
        .functions()
        .find(|function| function.name() == "readSeed")
        .expect("handle body")
        .entry()
        .symbol
        .clone();
    let selected = admit::entries(&mir, &[&name]).expect("selected body");
    let target = crate::select_object_target(crate::NATIVE_OBJECT_TARGET).expect("exact target");
    let bytes = emit::object(&mir, &selected, target).expect("real object");
    let file = object::File::parse(bytes.as_slice()).expect("independent ELF read");
    let sections = usize::try_from(u64_at(&bytes, 40)).expect("bounded header");
    let text = file.section_by_name(".text").expect("text");
    let text_header = sections + text.index().0 * 64;
    let rela = file.section_by_name(".rela.text").expect("calls");
    let (rela_offset, _) = rela.file_range().expect("readable relocations");
    let rela_offset = usize::try_from(rela_offset).expect("bounded relocation offset");
    let symtab = file.section_by_name(".symtab").expect("symbols");
    let (sym_offset, sym_size) = symtab.file_range().expect("readable symbols");
    let sym_offset = usize::try_from(sym_offset).expect("bounded symbol offset");
    let mut cases = Vec::new();
    let mut changed = bytes.clone();
    changed[4] = 1; // ELF32 claim over a 64-bit body.
    cases.push(changed);
    let mut changed = bytes.clone();
    changed[text_header + 8] |= 1; // Writable executable text.
    cases.push(changed);
    let mut changed = bytes.clone();
    changed[text_header + 32..text_header + 40].copy_from_slice(&u64::MAX.to_le_bytes());
    cases.push(changed);
    let mut changed = bytes.clone();
    changed[rela_offset + 16..rela_offset + 24].copy_from_slice(&42_i64.to_le_bytes());
    cases.push(changed);
    let mut changed = bytes.clone();
    changed[rela_offset + 8..rela_offset + 12]
        .copy_from_slice(&object::elf::R_X86_64_NONE.to_le_bytes());
    cases.push(changed);
    let mut changed = bytes.clone();
    changed[rela_offset..rela_offset + 8].copy_from_slice(&text.size().to_le_bytes());
    cases.push(changed);
    // Fixed ELF64 symbol records: independently turn a defined hidden function public,
    // and an undefined imported operation weak. Neither changes the source issuer.
    let symbols = (1..usize::try_from(sym_size / 24).expect("bounded symbols"))
        .map(|index| sym_offset + index * 24)
        .collect::<Vec<_>>();
    let defined =
        *symbols.iter().find(|offset| bytes[**offset + 4] == 0x12).expect("hidden definition");
    let mut changed = bytes.clone();
    changed[defined + 5] = 0;
    cases.push(changed);
    let imported = *symbols.iter().find(|offset| bytes[**offset + 4] == 0x10).expect("C import");
    let mut changed = bytes.clone();
    changed[imported + 4] = 0x20;
    cases.push(changed);
    for changed in cases {
        assert_eq!(
            audit::check(&changed, &mir, &selected)
                .expect_err("independent malformed object")
                .code(),
            "ZRYNA-N3003"
        );
    }
}
