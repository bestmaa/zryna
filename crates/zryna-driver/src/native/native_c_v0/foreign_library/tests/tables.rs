//! Mutations below the generic object iterator, including silently omitted relocation arrays.

use super::*;
use object::{Object as _, ObjectSection as _, write::Relocation};

#[test]
fn foreign_raw_headers_and_relocation_tables_reject_omitted_or_malformed_entries() {
    let requirements = linked_requirements(&capture::reference(), "readSeed");
    let valid = synthetic(|object| {
        let text = object.section_id(object::write::StandardSection::Text);
        let symbol = undefined(object, b"malloc");
        object
            .add_relocation(
                text,
                Relocation {
                    offset: 16,
                    symbol,
                    addend: -4,
                    flags: object::RelocationFlags::Elf { r_type: object::elf::R_X86_64_PLT32 },
                },
            )
            .expect("independent fixture prerequisite");
    });
    let file = object::File::parse(valid.as_slice()).expect("independent fixture prerequisite");
    let table = usize::try_from(u64::from_le_bytes(
        valid[40..48].try_into().expect("independent fixture prerequisite"),
    ))
    .expect("bounded header offset");
    let rel = table
        + file.section_by_name(".rela.text").expect("independent fixture prerequisite").index().0
            * 64;
    let text = table
        + file.section_by_name(".text").expect("independent fixture prerequisite").index().0 * 64;
    let symbol_data = usize::try_from(
        file.section_by_name(".symtab")
            .expect("independent fixture prerequisite")
            .file_range()
            .expect("independent fixture prerequisite")
            .0,
    )
    .expect("bounded symbol offset");
    let rel_data = file
        .section_by_name(".rela.text")
        .expect("independent fixture prerequisite")
        .file_range()
        .expect("independent fixture prerequisite")
        .0;
    let sym = table
        + file.section_by_name(".symtab").expect("independent fixture prerequisite").index().0 * 64;
    for (offset, width, value) in [
        (6, 1, 0),
        (7, 1, 3),
        (8, 1, 1),
        (9, 1, 1),
        (20, 4, 2),
        (24, 8, 1),
        (32, 8, 64),
        (48, 4, 1),
        (52, 2, 63),
        (54, 2, 56),
        (56, 2, 1),
        (58, 2, 63),
        (table, 4, 1),
        (rel + 4, 4, 1),
        (rel + 40, 4, 0),
        (rel + 44, 4, 0),
        (
            rel + 44,
            4,
            file.section_by_name(".symtab").expect("independent fixture prerequisite").index().0
                as u64,
        ),
        (rel + 56, 8, 8),
        (rel + 32, 8, 23),
        (rel + 24, 8, 0),
        (rel + 24, 8, rel_data + 1),
        (text + 24, 8, 0),
        (sym + 24, 8, table as u64),
        (symbol_data, 4, 1),
        (sym + 40, 4, 0),
        (sym + 44, 4, 0),
        (sym + 56, 8, 8),
        (sym + 32, 8, 25),
    ] {
        let mut changed = valid.clone();
        changed[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
        reject(&requirements, &changed, &["malloc"]);
        accept(&requirements, &valid, &["malloc"]);
    }
    let (offset, _) = file
        .section_by_name(".rela.text")
        .expect("independent fixture prerequisite")
        .file_range()
        .expect("independent fixture prerequisite");
    for target in [0, u32::MAX] {
        let mut changed = valid.clone();
        changed[usize::try_from(offset).expect("bounded relocation offset") + 12
            ..usize::try_from(offset).expect("bounded relocation offset") + 16]
            .copy_from_slice(&target.to_le_bytes());
        reject(&requirements, &changed, &["malloc"]);
    }
}
