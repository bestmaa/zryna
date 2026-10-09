//! Independently emitted inventories exercise the exact accepted bound and first extra record.

use super::*;
use object::write::Relocation;

#[test]
fn foreign_symbol_inventory_limit_accepts_exact_bound_and_rejects_first_extra() {
    let requirements = linked_requirements(&capture::reference(), "readSeed");
    for count in [audit::MAX_SYMBOLS, audit::MAX_SYMBOLS + 1] {
        let bytes = synthetic(|object| {
            let text = object.section_id(object::write::StandardSection::Text);
            object.append_section_data(text, &vec![0xc3; count], 1);
            for index in DEFINITIONS.len()..count {
                object.add_symbol(Symbol {
                    name: format!("local_{index}").into_bytes(),
                    value: index as u64,
                    size: 1,
                    kind: SymbolKind::Text,
                    scope: SymbolScope::Compilation,
                    weak: false,
                    section: SymbolSection::Section(text),
                    flags: SymbolFlags::None,
                });
            }
        });
        if count == audit::MAX_SYMBOLS {
            accept(&requirements, &bytes, &[]);
        } else {
            assert_eq!(
                capture_foreign_library(
                    &requirements,
                    &material_input(&requirements, &bytes, &sha(&bytes), &[])
                )
                .expect_err("hostile foreign input rejects")
                .code(),
                "ZRYNA-C4105"
            );
        }
    }
}

#[test]
fn foreign_relocation_limit_accepts_exact_bound_and_rejects_first_extra() {
    let requirements = linked_requirements(&capture::reference(), "readSeed");
    for count in [audit::MAX_RELOCATIONS, audit::MAX_RELOCATIONS + 1] {
        let bytes = synthetic(|object| {
            let text = object.section_id(object::write::StandardSection::Text);
            object.append_section_data(text, &vec![0; count * 4], 1);
            let symbol = undefined(object, b"malloc");
            for index in 0..count {
                object
                    .add_relocation(
                        text,
                        Relocation {
                            offset: 16 + index as u64 * 4,
                            symbol,
                            addend: -4,
                            flags: object::RelocationFlags::Elf {
                                r_type: object::elf::R_X86_64_PLT32,
                            },
                        },
                    )
                    .expect("independent fixture prerequisite");
            }
        });
        if count == audit::MAX_RELOCATIONS {
            accept(&requirements, &bytes, &["malloc"]);
        } else {
            assert_eq!(
                capture_foreign_library(
                    &requirements,
                    &material_input(&requirements, &bytes, &sha(&bytes), &["malloc"])
                )
                .expect_err("hostile foreign input rejects")
                .code(),
                "ZRYNA-C4105"
            );
        }
    }
}
