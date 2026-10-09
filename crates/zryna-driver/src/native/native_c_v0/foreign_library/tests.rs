//! Independent hostile ELF producers and real separately compiled C execution.

use super::*;
use object::{
    Architecture, BinaryFormat, Endianness, SectionKind, SymbolFlags, SymbolKind, SymbolScope,
    write::{Object, Symbol, SymbolSection},
};

use super::super::test_capture as capture;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod execution;
mod limits;
mod relocations;
mod support;
mod tables;

use support::*;

#[test]
fn foreign_snapshot_retains_actual_bytes_and_original_issuer() {
    let requirements = linked_requirements(&capture::reference(), "readSeed");
    let mut bytes = synthetic(|_| {});
    let captured = accept(&requirements, &bytes, &[]);
    let expected = bytes.clone();
    bytes.fill(0);
    drop(bytes);
    assert_eq!(captured.bytes(), expected);
    assert_eq!(captured.definitions().collect::<Vec<_>>(), DEFINITIONS);
    assert_eq!(captured.dependencies().len(), 0);
    captured.check_binding(&requirements.clone()).expect("same original issuer");
    let equal = linked_requirements(&capture::reference(), "readSeed");
    assert_eq!(requirements.object_sha256(), equal.object_sha256());
    assert_eq!(requirements.declaration_sha256(), equal.declaration_sha256());
    assert!(captured.check_binding(&equal).is_err(), "equal bytes cannot replace source issuer");
}

#[test]
fn foreign_identity_substitutions_reject_before_any_staging() {
    let requirements = linked_requirements(&capture::reference(), "readSeed");
    let bytes = synthetic(|_| {});
    let digest = sha(&bytes);
    let input = material_input(&requirements, &bytes, &digest, &[]);
    for changed in [
        ForeignLibraryInput {
            library_id: "fixture-c-v0@1",
            ..material_input(&requirements, &bytes, &digest, &[])
        },
        ForeignLibraryInput {
            library_id: "other@0",
            ..material_input(&requirements, &bytes, &digest, &[])
        },
        ForeignLibraryInput {
            header_bytes: b"same filename is not the original header",
            ..material_input(&requirements, &bytes, &digest, &[])
        },
        ForeignLibraryInput {
            policy_bytes: b"{}\n",
            ..material_input(&requirements, &bytes, &digest, &[])
        },
        ForeignLibraryInput {
            object_size: bytes.len() + 1,
            ..material_input(&requirements, &bytes, &digest, &[])
        },
        ForeignLibraryInput {
            object_sha256: &[0; 32],
            ..material_input(&requirements, &bytes, &digest, &[])
        },
    ] {
        assert_eq!(
            capture_foreign_library(&requirements, &changed)
                .expect_err("hostile foreign input rejects")
                .code(),
            "ZRYNA-C4102"
        );
        capture_foreign_library(&requirements, &input).expect("pristine recovery");
    }
    let mut changed = bytes.clone();
    changed[64] ^= 1;
    let changed = material_input(&requirements, &changed, &digest, &[]);
    assert!(capture_foreign_library(&requirements, &changed).is_err());
}

#[test]
fn foreign_elf_class_target_and_kind_are_independent_linked_requirements() {
    let requirements = linked_requirements(&capture::reference(), "imported");
    let valid = synthetic(|_| {});
    for (offset, value) in [(4, 1), (5, 2), (16, 2), (16, 3), (18, 3), (18, 183)] {
        let mut changed = valid.clone();
        changed[offset] = value;
        reject(&requirements, &changed, &[]);
    }
    for changed in [&valid[..16], b"!<arch>\n", b"not ELF"] {
        reject(&requirements, changed, &[]);
    }
    accept(&requirements, &valid, &[]);
}

#[test]
fn foreign_closed_global_inventory_rejects_names_kinds_weak_common_and_aliases() {
    let requirements = linked_requirements(&capture::reference(), "readSeed");
    for mutation in 0..9 {
        let bytes = synthetic(|object| {
            let text = object.section_id(object::write::StandardSection::Text);
            let symbol =
                object.symbol_mut(object.symbol_id(b"fixture_read").expect("independent symbol"));
            match mutation {
                0 => symbol.name = b"renamed_read".to_vec(),
                1 => symbol.kind = SymbolKind::Data,
                2 => symbol.weak = true,
                3 => symbol.section = SymbolSection::Common,
                4 => symbol.flags = SymbolFlags::Elf { st_info: 0x1a, st_other: 0 },
                5 => symbol.value = 0,
                6 => {
                    object.add_symbol(Symbol {
                        name: b"ZRYNA_c_v0_i_dispatch".to_vec(),
                        value: 7,
                        size: 1,
                        kind: SymbolKind::Text,
                        scope: SymbolScope::Linkage,
                        weak: false,
                        section: SymbolSection::Section(text),
                        flags: SymbolFlags::None,
                    });
                }
                7 => {
                    object.add_symbol(Symbol {
                        name: b"Add".to_vec(),
                        value: 7,
                        size: 1,
                        kind: SymbolKind::Text,
                        scope: SymbolScope::Linkage,
                        weak: false,
                        section: SymbolSection::Section(text),
                        flags: SymbolFlags::None,
                    });
                }
                _ => {
                    object.add_symbol(Symbol {
                        name: b"fixture_read".to_vec(),
                        value: 7,
                        size: 1,
                        kind: SymbolKind::Text,
                        scope: SymbolScope::Linkage,
                        weak: false,
                        section: SymbolSection::Section(text),
                        flags: SymbolFlags::None,
                    });
                }
            }
        });
        reject(&requirements, &bytes, &[]);
        accept(&requirements, &synthetic(|_| {}), &[]);
    }
}

#[test]
fn foreign_dependencies_are_exact_strong_ordered_and_not_private_runtime_symbols() {
    let requirements = linked_requirements(&capture::reference(), "readSeed");
    let bytes = synthetic(|object| {
        undefined(object, b"free");
        undefined(object, b"malloc");
    });
    accept(&requirements, &bytes, &["free", "malloc"]);
    for names in [
        &[][..],
        &["malloc"][..],
        &["malloc", "free"][..],
        &["free", "free", "malloc"][..],
        &["free", "malloc", "system"][..],
    ] {
        reject(&requirements, &bytes, names);
    }
    let private = synthetic(|object| {
        undefined(object, b"zryna_rt_o1_release");
    });
    reject(&requirements, &private, &["zryna_rt_o1_release"]);
    let weak = synthetic(|object| {
        let id = undefined(object, b"malloc");
        object.symbol_mut(id).weak = true;
    });
    reject(&requirements, &weak, &["malloc"]);
}

#[test]
fn foreign_dynamic_constructor_tls_and_open_section_inventories_reject() {
    let requirements = linked_requirements(&capture::reference(), "readSeed");
    for name in
        [".init_array", ".fini_array", ".ctors", ".dynamic", ".tdata", ".eh_frame", ".debug_info"]
    {
        let bytes = synthetic(|object| {
            let id = object.add_section(Vec::new(), name.as_bytes().to_vec(), SectionKind::Data);
            object.append_section_data(id, &[0; 8], 8);
        });
        reject(&requirements, &bytes, &[]);
    }
    let bytes = synthetic(|object| {
        let id = object.section_id(object::write::StandardSection::Text);
        object.section_mut(id).flags = object::SectionFlags::Elf { sh_flags: 7 };
    });
    reject(&requirements, &bytes, &[]);
}

#[test]
fn foreign_artifact_and_allocated_storage_bounds_reject_before_materialization() {
    let requirements = linked_requirements(&capture::reference(), "readSeed");
    let bytes = vec![0; zryna_backend_native::MAX_NATIVE_OBJECT_BYTES + 1];
    let digest = sha(&bytes);
    assert_eq!(
        capture_foreign_library(
            &requirements,
            &material_input(&requirements, &bytes, &digest, &[])
        )
        .expect_err("hostile foreign input rejects")
        .code(),
        "ZRYNA-C4105"
    );
    let bytes = synthetic(|object| {
        let id = object.section_id(object::write::StandardSection::UninitializedData);
        object.append_section_bss(
            id,
            (zryna_backend_native::MAX_NATIVE_OBJECT_BYTES + 1) as u64,
            1,
        );
    });
    reject(&requirements, &bytes, &[]);
    accept(&requirements, &synthetic(|_| {}), &[]);
}
