//! Genuine source-to-machine recapture and independently malformed object witnesses.

#[path = "../../../zryna-native-mir/tests/native_c_v0/capture.rs"]
mod capture;

use super::*;
use object::{Object, ObjectSymbol};

fn emit() -> ValidatedScalarExports {
    let (_, source) = capture::reference();
    let machine = zryna_native_mir::native_c_v0::lower(&source).expect("genuine machine seal");
    emit_scalar_exports(
        &machine,
        crate::select_object_target(NATIVE_OBJECT_TARGET).expect("exact target"),
    )
    .expect("audited total scalar export object")
}

#[test]
fn every_total_export_has_exact_symbol_header_and_retained_original_authority() {
    let extra_import = capture::boolean_import();
    assert_eq!(extra_import.operations().len(), 9);
    let artifact = emit();
    let file = object::File::parse(artifact.bytes()).expect("ELF");
    let public = file
        .symbols()
        .filter(ObjectSymbol::is_global)
        .map(|symbol| symbol.name().expect("exact bytes").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(public, ["zryna_c_v0_e_add"]);
    assert!(artifact.header().contains("int32_t zryna_c_v0_e_add(int32_t arg0, int32_t arg1);"));
    assert!(!artifact.header().contains("fixture_open"));
    assert!(!artifact.header().contains("zryna_c_v0_i_dispatch"));
    assert_eq!(artifact.program().functions().len(), 6);
    assert_eq!(artifact.program().operations().len(), 8);
    let repeated = emit_scalar_exports(
        artifact.program(),
        crate::select_object_target(NATIVE_OBJECT_TARGET).expect("target"),
    )
    .expect("deterministic emission");
    assert_eq!(artifact.bytes(), repeated.bytes());
    assert_eq!(artifact.header(), repeated.header());
}

#[test]
fn original_seven_eight_nine_and_sixteen_argument_exports_keep_their_complete_headers() {
    for count in [7, 8, 9, 16] {
        let source = capture::stack_export(count);
        let machine = zryna_native_mir::native_c_v0::lower(&source).expect("stack machine lanes");
        let artifact = emit_scalar_exports(
            &machine,
            crate::select_object_target(NATIVE_OBJECT_TARGET).expect("target"),
        )
        .expect("ABI stack exports");
        assert!(artifact.header().contains(&format!("int32_t arg{});", count - 1)));
        assert!(!artifact.header().contains(&format!("arg{count}")));
        audit::check(artifact.bytes(), &machine).expect("complete independent symbol audit");
    }
}

#[test]
fn bool32_and_c_int_headers_keep_declared_spelling_and_objects_are_audited() {
    for (source, declaration) in [
        (capture::constant_export(), "int32_t zryna_c_v0_e_add(void);"),
        (capture::boolean_export(), "uint32_t zryna_c_v0_e_add(uint32_t arg0);"),
        (capture::c_int_export(), "int zryna_c_v0_e_add(int arg0, int arg1);"),
    ] {
        let machine = zryna_native_mir::native_c_v0::lower(&source).expect("genuine scalar seal");
        let artifact = emit_scalar_exports(
            &machine,
            crate::select_object_target(NATIVE_OBJECT_TARGET).expect("target"),
        )
        .expect("audited exact spelling");
        assert!(artifact.header().contains(declaration));
    }
}

#[test]
fn wrong_elf_identity_symbol_kind_visibility_section_flags_and_missing_definition_reject() {
    let artifact = emit();
    for (offset, byte) in [(0, 0), (4, 1), (5, 2), (16, 3), (18, 3)] {
        let mut bytes = artifact.bytes().to_vec();
        bytes[offset] = byte;
        assert_eq!(
            audit::check(&bytes, artifact.program()).expect_err("no false ELF").code(),
            "ZRYNA-N3003"
        );
    }
    let symbol_table = section(artifact.bytes(), b".symtab");
    let bytes = artifact.bytes();
    let start = usize::try_from(u64_at(bytes, symbol_table + 24)).expect("symbol table offset");
    let size = usize::try_from(u64_at(bytes, symbol_table + 32)).expect("symbol table size");
    let public =
        (start..start + size).step_by(24).find(|at| bytes[at + 4] == 0x12).expect("export record");
    for (offset, byte) in [(public + 4, 0x11), (public + 5, 2), (public + 6, 0)] {
        let mut bytes = bytes.to_vec();
        bytes[offset] = byte;
        assert!(
            audit::check(&bytes, artifact.program()).is_err(),
            "changed ELF symbol at {offset}"
        );
    }
    let file_symbol = (start..start + size)
        .step_by(24)
        .find(|at| bytes[at + 4] == 4)
        .expect("fixed file metadata");
    for (offset, byte) in [(file_symbol + 4, 3), (file_symbol + 5, 2)] {
        let mut forged = bytes.to_vec();
        forged[offset] = byte;
        assert!(audit::check(&forged, artifact.program()).is_err());
    }
    let mut forged = bytes.to_vec();
    let text = section(bytes, b".text");
    forged[text + 8] = 7; // SHF_WRITE must never be added to executable code.
    assert!(audit::check(&forged, artifact.program()).is_err());
    let mut unreadable = bytes.to_vec();
    unreadable[text + 24..text + 32]
        .copy_from_slice(&u64::try_from(bytes.len()).expect("object size").to_le_bytes());
    assert!(audit::check(&unreadable, artifact.program()).is_err(), "truncated text payload");
    let mut renamed = bytes.to_vec();
    let symbol = b"zryna_c_v0_e_add\0";
    let name = renamed.windows(symbol.len()).position(|w| w == symbol).expect("exact symbol");
    renamed[name + 13] = b'x';
    assert!(audit::check(&renamed, artifact.program()).is_err());
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("ELF64 integer"))
}
fn section(bytes: &[u8], name: &[u8]) -> usize {
    let table = usize::try_from(u64_at(bytes, 40)).expect("section offset");
    let count = usize::from(u16::from_le_bytes(bytes[60..62].try_into().expect("section count")));
    let names = usize::from(u16::from_le_bytes(bytes[62..64].try_into().expect("names index")));
    let string_table =
        usize::try_from(u64_at(bytes, table + names * 64 + 24)).expect("names offset");
    (0..count)
        .map(|index| table + index * 64)
        .find(|header| {
            let offset = usize::try_from(u32::from_le_bytes(
                bytes[*header..*header + 4].try_into().expect("name offset"),
            ))
            .expect("offset");
            let start = string_table + offset;
            bytes[start..].split(|byte| *byte == 0).next() == Some(name)
        })
        .expect("exact section")
}
