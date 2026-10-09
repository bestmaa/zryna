use super::{FlowStep, capture, verify_bodies};

#[test]
fn native_c_body_v0_scalar_status_alias_preserves_exact_call_and_empty_recoverable_guard_is_valid()
{
    let handle = capture::HANDLE.replace(
        "  if (opened !== 0) { return Ffi.foreignError(\"fixture-c-v0@0/fixture_open\", opened); }",
        "  const alias: i32 = opened;\n  if (alias !== 0) { return Ffi.foreignError(\"fixture-c-v0@0/fixture_open\", alias); }");
    let capture = capture::edited(capture::BUFFER, &handle, capture::SCALAR);
    let bodies = verify_bodies(&capture.sources, &capture.declarations).expect("exact call alias");
    let read = bodies
        .functions()
        .iter()
        .find(|function| function.name() == "readSeed")
        .expect("handle wrapper");
    let guards: Vec<_> = read
        .steps()
        .iter()
        .filter_map(|step| match step {
            FlowStep::StatusGuard { call, recoverable, .. } => Some((*call, recoverable)),
            _ => None,
        })
        .collect();
    assert_eq!(guards.len(), 2);
    assert_eq!(guards[0].1.as_slice(), [1, 2]);
    assert!(guards[1].1.is_empty(), "fixture_read has no recoverable status");
    assert_ne!(guards[0].0, guards[1].0);
    assert!(read.steps().iter().any(|step| matches!(step,
        FlowStep::Call { call, recoverable, unknown_status_unresolved_owners, .. }
            if *call == guards[1].0 && recoverable.is_empty() && unknown_status_unresolved_owners.is_empty()
    )), "unknown read status is a call-boundary failure, never a ForeignError");
}

#[test]
fn native_c_body_v0_guards_reject_equal_scalar_forged_status_and_wrong_operation_provenance() {
    let guard =
        "  if (opened !== 0) { return Ffi.foreignError(\"fixture-c-v0@0/fixture_open\", opened); }";
    for (replacement, detail) in [
        (
            "  if (seed !== 0) { return Ffi.foreignError(\"fixture-c-v0@0/fixture_open\", opened); }",
            "guard-status-provenance",
        ),
        (
            "  if (opened !== 0) { return Ffi.foreignError(\"fixture-c-v0@0/fixture_open\", 1); }",
            "foreign-error-status-provenance",
        ),
        (
            "  if (opened !== 0) { return Ffi.foreignError(\"fixture-c-v0@0/fixture_read\", opened); }",
            "foreign-error-operation-provenance",
        ),
        ("", "output-before-exact-status-zero"),
    ] {
        let handle = capture::HANDLE.replace(guard, replacement);
        let capture = capture::edited(capture::BUFFER, &handle, capture::SCALAR);
        assert_eq!(
            verify_bodies(&capture.sources, &capture.declarations).expect_err(detail).detail(),
            detail
        );
    }
}

#[test]
fn native_c_body_v0_same_operation_other_call_never_initializes_the_first_calls_slot() {
    let extra = r#"
function two(): i32 {
  const first: FfiHandleOut = Ffi.outHandle("fixture-c-v0@0/fixture_handle");
  const a: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_open", 1, first);
  const second: FfiHandleOut = Ffi.outHandle("fixture-c-v0@0/fixture_handle");
  const b: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_open", 2, second);
  if (b !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_open", b); }
  const wrong: FfiHandle = Ffi.takeHandle(first);
  if (a !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_open", a); }
  return 0;
}
"#;
    let capture = capture::append_handle(extra);
    assert_eq!(
        verify_bodies(&capture.sources, &capture.declarations)
            .expect_err("same key is not same call")
            .detail(),
        "output-before-exact-status-zero"
    );
}

#[test]
fn native_c_body_v0_unread_outputs_and_unhandled_status_cannot_become_normal_values() {
    for (extra, detail) in [
        (
            "function early(): i32 { const out: FfiI32Out = Ffi.outI32(); return Ffi.readI32(out); }",
            "output-before-call",
        ),
        (
            "function early(): i32 { const out: FfiHandleOut = Ffi.outHandle(\"fixture-c-v0@0/fixture_handle\"); const status: i32 = Ffi.rawCall(\"fixture-c-v0@0/fixture_open\", 0, out); return status; }",
            "unhandled-call-status",
        ),
        (
            "function early(): i32 { const out: FfiHandleOut = Ffi.outHandle(\"fixture-c-v0@0/fixture_handle\"); const status: i32 = Ffi.rawCall(\"fixture-c-v0@0/fixture_open\", 0, out); return status + 1; }",
            "unchecked-status-arithmetic",
        ),
    ] {
        let capture = capture::append_handle(extra);
        assert_eq!(
            verify_bodies(&capture.sources, &capture.declarations).expect_err(detail).detail(),
            detail
        );
    }
}

#[test]
fn native_c_body_v0_byte_output_pair_retains_exact_call_and_resource_group() {
    let extra = r#"
function pairs(bytes: Vec<i32>): i32 {
  const loan: FfiBytes = Ffi.borrowBytes(bytes);
  const p1: FfiBytesOut = Ffi.outBytes(); const n1: FfiCountOut = Ffi.outCount();
  const a: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_copy_bytes", loan, Ffi.byteLength(loan), p1, n1);
  if (a !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_copy_bytes", a); }
  const p2: FfiBytesOut = Ffi.outBytes(); const n2: FfiCountOut = Ffi.outCount();
  const b: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_copy_bytes", loan, Ffi.byteLength(loan), p2, n2);
  if (b !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_copy_bytes", b); }
  const wrong: FfiOwnedBytes = Ffi.takeBytes(p1, n2);
  return 0;
}
"#;
    let capture = capture::append_handle(extra);
    assert_eq!(
        verify_bodies(&capture.sources, &capture.declarations)
            .expect_err("wrong exact output pair")
            .detail(),
        "mismatched-byte-output-pair"
    );
}
