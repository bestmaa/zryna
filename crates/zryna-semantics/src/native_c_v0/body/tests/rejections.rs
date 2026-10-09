use super::{capture, verify_bodies};

fn reject(buffer: &str, handle: &str, scalar: &str, detail: &str) {
    let capture = capture::edited(buffer, handle, scalar);
    let error = verify_bodies(&capture.sources, &capture.declarations).expect_err(detail);
    assert_eq!(error.detail(), detail);
    let span = error.span().expect("body error uses original authenticated source");
    assert!(capture.sources.resolve(span).is_ok());
    let valid = capture::reference();
    assert!(
        verify_bodies(&valid.sources, &valid.declarations).is_ok(),
        "valid successor has no stale rejection state"
    );
}

#[test]
fn native_c_body_v0_rejects_wrong_source_types_and_unresolved_names_after_declaration_admission() {
    let rows = [
        (
            capture::SCALAR.replace(
                "Ffi.rawCall(\"fixture-c-v0@0/add\", left, right)",
                "Ffi.rawCall(\"fixture-c-v0@0/add\", true, right)",
            ),
            "raw-call-argument-type",
        ),
        (
            capture::SCALAR.replace(
                "Ffi.rawCall(\"fixture-c-v0@0/add\", left, right)",
                "Ffi.rawCall(\"fixture-c-v0@0/add\", missing, right)",
            ),
            "unknown-local",
        ),
        (capture::SCALAR.replace("return left + right;", "return true + right;"), "addition-type"),
        (capture::SCALAR.replace("return left + right;", "return true;"), "function-return-type"),
        (capture::SCALAR.replace("return left + right;", "return missing;"), "unknown-local"),
    ];
    for (scalar, detail) in rows {
        reject(capture::BUFFER, capture::HANDLE, &scalar, detail);
    }
    let handle = capture::HANDLE.replace("const value: FfiI32Out", "const value: bool");
    reject(capture::BUFFER, &handle, capture::SCALAR, "binding-type");
    let buffer = capture::BUFFER.replace("Ffi.borrowBytes(bytes)", "Ffi.borrowUtf8(bytes)");
    reject(&buffer, capture::HANDLE, capture::SCALAR, "loan-source-type");
}

#[test]
fn native_c_body_v0_foreign_tokens_never_cross_even_private_function_boundaries() {
    for (extra, detail) in [
        ("function escaped(owner: FfiHandle): i32 { return 0; }", "foreign-token-parameter"),
        ("function escaped(loan: FfiBytes): i32 { return 0; }", "foreign-token-parameter"),
        ("function escaped(out: FfiI32Out): i32 { return 0; }", "foreign-token-parameter"),
        (
            "function escaped(bytes: Vec<i32>): FfiBytes { return Ffi.borrowBytes(bytes); }",
            "foreign-token-return",
        ),
        ("function escaped(): FfiHandle { return 0; }", "foreign-token-return"),
    ] {
        let capture = capture::append_handle(extra);
        assert_eq!(
            verify_bodies(&capture.sources, &capture.declarations).expect_err(detail).detail(),
            detail
        );
    }
}

#[test]
fn native_c_body_v0_complete_body_rejects_missing_return_unreachable_occupants_and_duplicate_locals()
 {
    for (extra, detail) in [
        ("function missing(): i32 { 0; }", "missing-terminal-return"),
        (
            "function duplicate(): i32 { const value: i32 = 0; const value: i32 = 1; return value; }",
            "duplicate-body-binding",
        ),
        ("function tail(): i32 { return 0; Ffi.outI32(); }", "statement-after-terminal-return"),
    ] {
        let capture = capture::append_handle(extra);
        assert_eq!(
            verify_bodies(&capture.sources, &capture.declarations).expect_err(detail).detail(),
            detail
        );
    }
}

#[test]
fn native_c_body_v0_tokens_move_once_and_release_invalidates_every_reference() {
    let handle = capture::HANDLE.replace(
        "  const value: FfiI32Out",
        "  const moved: FfiHandle = handle;\n  const value: FfiI32Out",
    );
    reject(capture::BUFFER, &handle, capture::SCALAR, "moved-binding");
    let handle = capture::HANDLE.replace(
        "  const value: FfiI32Out",
        "  const again: FfiHandle = Ffi.takeHandle(out);\n  const value: FfiI32Out",
    );
    reject(capture::BUFFER, &handle, capture::SCALAR, "stale-token");
    let release = "  Ffi.release(\"fixture-c-v0@0/fixture_close\", handle);";
    let handle = capture::HANDLE.replace(release, &format!("{release}\n{release}"));
    reject(capture::BUFFER, &handle, capture::SCALAR, "stale-token");
    let handle = capture::HANDLE.replace(
        release,
        &format!("  Ffi.release(\"fixture-c-v0@0/fixture_release_bytes\", handle);\n{release}"),
    );
    reject(capture::BUFFER, &handle, capture::SCALAR, "raw-call-argument-type");
}

#[test]
fn native_c_body_v0_wrong_call_arity_and_nonhandle_kind_are_rejected_before_body_admission() {
    let scalar = capture::SCALAR.replace(
        "Ffi.rawCall(\"fixture-c-v0@0/add\", left, right)",
        "Ffi.rawCall(\"fixture-c-v0@0/add\", left)",
    );
    let handle = capture::HANDLE.replace(
        "Ffi.outHandle(\"fixture-c-v0@0/fixture_handle\")",
        "Ffi.outHandle(\"fixture-c-v0@0/owned_bytes\")",
    );
    for (handle, scalar, code, detail) in [
        (capture::HANDLE, scalar.as_str(), "ZRYNA-C4104", "raw-arity"),
        (handle.as_str(), capture::SCALAR, "ZRYNA-C4105", "unknown-handle-kind"),
    ] {
        let error = capture::try_edited(capture::BUFFER, handle, scalar)
            .expect_err("mandatory declaration rejection precedes the body entry");
        assert_eq!((error.code(), error.detail()), (code, detail));
    }
    let valid = capture::reference();
    assert!(verify_bodies(&valid.sources, &valid.declarations).is_ok());
}
