use super::*;
use object::RelocationFlags;
use object::write::Relocation;

#[test]
fn foreign_relocations_reject_wrong_target_type_addend_bounds_and_overlap() {
    let requirements = linked_requirements(&capture::reference(), "readSeed");
    for mutation in 0..7 {
        let bytes = synthetic(|object| {
            let text = object.section_id(object::write::StandardSection::Text);
            let external = undefined(object, b"malloc");
            let kind = if mutation == 0 {
                object::elf::R_X86_64_GOTPCREL
            } else {
                object::elf::R_X86_64_PLT32
            };
            object
                .add_relocation(
                    text,
                    Relocation {
                        offset: if mutation == 1 { 63 } else { 16 },
                        symbol: if mutation == 2 {
                            object.symbol_id(b"add").expect("independent fixture prerequisite")
                        } else {
                            external
                        },
                        addend: if mutation == 3 { 0 } else { -4 },
                        flags: RelocationFlags::Elf { r_type: kind },
                    },
                )
                .expect("independent relocation");
            if mutation == 4 {
                object
                    .add_relocation(
                        text,
                        Relocation {
                            offset: 18,
                            symbol: external,
                            addend: -4,
                            flags: RelocationFlags::Elf { r_type: object::elf::R_X86_64_PLT32 },
                        },
                    )
                    .expect("independent fixture prerequisite");
            }
            if mutation == 5 {
                object.symbol_mut(external).flags = SymbolFlags::Elf { st_info: 0x11, st_other: 0 };
            }
            if mutation == 6 {
                object.symbol_mut(external).name = b"undeclared".to_vec();
            }
        });
        if mutation == 2 {
            accept(&requirements, &bytes, &["malloc"]);
        } else {
            reject(&requirements, &bytes, &["malloc"]);
        }
    }
    let accepted = synthetic(|object| {
        let text = object.section_id(object::write::StandardSection::Text);
        let id = undefined(object, b"malloc");
        object
            .add_relocation(
                text,
                Relocation {
                    offset: 16,
                    symbol: id,
                    addend: -4,
                    flags: RelocationFlags::Elf { r_type: object::elf::R_X86_64_PLT32 },
                },
            )
            .expect("independent fixture prerequisite");
    });
    accept(&requirements, &accepted, &["malloc"]);
}

#[test]
fn foreign_relocations_reject_section_pointer_escape_and_wrong_static_data_range() {
    let requirements = linked_requirements(&capture::reference(), "readSeed");
    for (offset, addend, kind) in [
        (16, 0, object::elf::R_X86_64_32),
        (16, 8, object::elf::R_X86_64_32),
        (16, -1, object::elf::R_X86_64_32),
        (63, 0, object::elf::R_X86_64_64),
        (16, -4, object::elf::R_X86_64_PLT32),
    ] {
        let bytes = synthetic(|object| {
            let text = object.section_id(object::write::StandardSection::Text);
            let data = object.section_id(object::write::StandardSection::ReadOnlyData);
            object.append_section_data(data, &[42; 8], 1);
            let id = object.section_symbol(data);
            object
                .add_relocation(
                    text,
                    Relocation {
                        offset,
                        symbol: id,
                        addend,
                        flags: RelocationFlags::Elf { r_type: kind },
                    },
                )
                .expect("independent fixture prerequisite");
        });
        if offset == 16 && addend == 0 && kind == object::elf::R_X86_64_32 {
            accept(&requirements, &bytes, &[]);
        } else {
            reject(&requirements, &bytes, &[]);
        }
    }
}
