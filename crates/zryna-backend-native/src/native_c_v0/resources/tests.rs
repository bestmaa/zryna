//! Original sealed fixture through native private handle emission.

#[path = "../../../../zryna-native-c-ir/tests/capture.rs"]
mod capture;

use super::*;
mod bytes;
mod malformed;

fn fixture() -> VerifiedMirProgram {
    let capture = capture::compact_handle("");
    let ir = zryna_native_c_ir::lower(&capture.sources, &capture.authority).expect("genuine IR");
    zryna_native_mir::native_c_v0::lower(&ir).expect("genuine machine seal")
}

#[test]
fn handle_selection_rejects_unknown_duplicate_and_unsupported_private_storage() {
    let mir = fixture();
    let name = mir
        .functions()
        .find(|function| function.name() == "readSeed")
        .expect("handle body")
        .entry()
        .symbol
        .clone();
    for names in
        [vec![], vec![name.as_str(), name.as_str()], vec!["readSeed"], vec!["fixture_open"]]
    {
        assert_eq!(
            admit::entries(&mir, &names).expect_err("closed selection").code(),
            "ZRYNA-N3002"
        );
    }
    let unsupported = mir
        .functions()
        .find(|function| !function.private_owners().is_empty())
        .expect("complete seal retains byte body");
    assert_eq!(
        admit::entries(&mir, &[&unsupported.entry().symbol])
            .expect_err("unsupported storage")
            .code(),
        "ZRYNA-N3002"
    );
}

#[test]
fn native_handle_object_has_exact_private_entries_and_imports() {
    let original = capture::reference();
    let capture = capture::edited(capture::BUFFER, capture::HANDLE, capture::SCALAR);
    assert_eq!(
        original.authority.body_authority().declaration_sha256(),
        capture.authority.body_authority().declaration_sha256()
    );
    let ir = zryna_native_c_ir::lower(&capture.sources, &capture.authority).expect("genuine IR");
    let mir = zryna_native_mir::native_c_v0::lower(&ir).expect("genuine machine seal");
    let entry = mir
        .functions()
        .find(|function| function.name() == "readSeed")
        .expect("handle body")
        .entry()
        .symbol
        .clone();
    let target =
        crate::select_object_target(crate::NATIVE_OBJECT_TARGET).expect("exact Linux capability");
    let selected = admit::entries(&mir, &[&entry]).expect("closed executable body");
    let bytes = emit::object(&mir, &selected, target).expect("real object");
    audit::check(&bytes, &mir, &selected).expect("independent object audit");
    let artifact = emit_handle_entries(&mir, &[&entry], target).expect("audited handle artifact");
    assert_eq!(artifact.bytes(), bytes);
    assert!(artifact.header().contains(&entry));
    assert_eq!(artifact.entries(), selected);
}
