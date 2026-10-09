//! Exact byte inventories and hostile native fields, independent of the matching producer.

use super::*;
use object::{Object, ObjectSection};
use std::collections::BTreeSet;

#[test]
fn byte_artifacts_retain_complete_issuers_and_exact_private_runtime_imports() {
    let capture = capture::reference();
    let ir = zryna_native_c_ir::lower(&capture.sources, &capture.authority).expect("original IR");
    let mir = zryna_native_mir::native_c_v0::lower(&ir).expect("original MIR");
    for (name, runtime, foreign) in [
        (
            "sum",
            vec!["zryna_rt_o1_allocate", "zryna_rt_o1_release", "zryna_rt_o1_vec_release_storage"],
            vec!["sum_bytes"],
        ),
        ("utf8Sum", vec!["zryna_rt_o1_string_release"], vec!["sum_bytes"]),
        (
            "copied",
            vec![
                "zryna_rt_o1_allocate",
                "zryna_rt_o1_release",
                "zryna_rt_o1_vec_allocate",
                "zryna_rt_o1_vec_release_storage",
            ],
            vec!["fixture_copy_bytes", "fixture_release_bytes"],
        ),
    ] {
        let function =
            mir.functions().find(|function| function.name() == name).expect("original body");
        let target =
            crate::select_object_target(crate::NATIVE_OBJECT_TARGET).expect("exact target");
        assert!(emit_handle_entries(&mir, &[&function.entry().symbol], target).is_err());
        let artifact = emit_byte_entries(&mir, &[&function.entry().symbol], target)
            .expect("audited byte object");
        assert!(artifact.uses_storage_channel());
        assert!(artifact.program().source().belongs_to(&capture.sources));
        assert_eq!(artifact.program().functions().len(), mir.functions().len());
        assert_eq!(
            artifact
                .imported_runtime_operations()
                .map(zryna_ownership_runtime_abi::VerifiedNativeFunction::symbol)
                .collect::<BTreeSet<_>>(),
            runtime.into_iter().collect()
        );
        assert_eq!(
            artifact
                .imported_operations()
                .map(|operation| operation.declaration().symbol.as_str())
                .collect::<BTreeSet<_>>(),
            foreign.into_iter().collect()
        );
        assert!(artifact.header().contains("0x5a43425954455330"));
        assert!(artifact.header().contains("sizeof(struct zryna_c_v0_context) == 1568"));
    }
}

#[test]
fn byte_object_audit_rejects_foreign_runtime_symbols_and_mutated_call_fields() {
    let capture = capture::reference();
    let ir = zryna_native_c_ir::lower(&capture.sources, &capture.authority).expect("original IR");
    let mir = zryna_native_mir::native_c_v0::lower(&ir).expect("original MIR");
    let function = mir.functions().find(|function| function.name() == "copied").expect("byte body");
    let selected =
        admit::storage_entries(&mir, &[&function.entry().symbol]).expect("exact selection");
    let target = crate::select_object_target(crate::NATIVE_OBJECT_TARGET).expect("exact target");
    let bytes = emit::object(&mir, &selected, target).expect("real machine object");
    let file = object::File::parse(bytes.as_slice()).expect("independent ELF decode");
    let (strings, size) =
        file.section_by_name(".strtab").expect("names").file_range().expect("bytes");
    let strings = usize::try_from(strings).expect("bounded field");
    let size = usize::try_from(size).expect("bounded field");
    let name = b"zryna_rt_o1_allocate";
    let offset = bytes[strings..strings + size]
        .windows(name.len())
        .position(|candidate| candidate == name)
        .expect("required private import");
    let mut changed = bytes.clone();
    changed[strings + offset + name.len() - 1] = b'f';
    assert_eq!(
        audit::check(&changed, &mir, &selected).expect_err("unknown runtime name").code(),
        "ZRYNA-N3003"
    );
    let (relocations, _) =
        file.section_by_name(".rela.text").expect("calls").file_range().expect("bytes");
    let relocations = usize::try_from(relocations).expect("bounded field");
    let mut changed = bytes.clone();
    changed[relocations + 16..relocations + 24].copy_from_slice(&17_i64.to_le_bytes());
    assert_eq!(
        audit::check(&changed, &mir, &selected).expect_err("invalid call addend").code(),
        "ZRYNA-N3003"
    );
}
