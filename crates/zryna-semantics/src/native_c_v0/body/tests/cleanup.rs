use super::super::{CallEntry, ValueType};
use super::{FlowStep, TrapRequirement, capture, verify_bodies};

#[test]
fn native_c_body_v0_untaken_acquisition_prefix_cleans_in_reverse_and_failed_call_adds_no_owner() {
    let extra = r#"
function prefix(): i32 {
  const one: FfiHandleOut = Ffi.outHandle("fixture-c-v0@0/fixture_handle");
  const a: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_open", 1, one);
  const two: FfiHandleOut = Ffi.outHandle("fixture-c-v0@0/fixture_handle");
  const b: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_open", 2, two);
  if (b !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_open", b); }
  if (a !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_open", a); }
  return 0;
}
"#;
    let capture = capture::append_handle(extra);
    let bodies = verify_bodies(&capture.sources, &capture.declarations)
        .expect("conditional untaken outputs");
    let function = bodies
        .functions()
        .iter()
        .find(|function| function.name() == "prefix")
        .expect("prefix body");
    let owners = function.owner_origins();
    assert_eq!(owners.len(), 2);
    let guards: Vec<_> = function
        .steps()
        .iter()
        .filter_map(|step| match step {
            FlowStep::StatusGuard { call, cleanup, .. } => Some((*call, cleanup)),
            _ => None,
        })
        .collect();
    assert_eq!(
        guards[0].1.iter().map(super::super::cleanup::CleanupEntry::owner_id).collect::<Vec<_>>(),
        [0]
    );
    assert_eq!(
        guards[1].1.iter().map(super::super::cleanup::CleanupEntry::owner_id).collect::<Vec<_>>(),
        [1]
    );
    assert!(guards.iter().all(|(_, entries)| {
        entries.iter().all(super::super::cleanup::CleanupEntry::validation_required)
    }));
    assert!(matches!(function.steps().last(), Some(FlowStep::Return { cleanup, .. })
        if cleanup.iter().map(super::super::cleanup::CleanupEntry::owner_id).collect::<Vec<_>>() == [1, 0]));
    assert!(
        function
            .expressions()
            .iter()
            .all(|expression| expression.value_type() != ValueType::Handle)
    );
}

#[test]
fn native_c_body_v0_copy_trap_retains_foreign_owner_and_safe_empty_release_is_not_a_second_c_call()
{
    let capture = capture::reference();
    let bodies = verify_bodies(&capture.sources, &capture.declarations)
        .expect("complete private copy requirements");
    let copied = bodies
        .functions()
        .iter()
        .find(|function| function.name() == "copied")
        .expect("byte copy wrapper");
    assert!(copied.steps().iter().any(|step| matches!(step,
        FlowStep::Copy { owner: 0, trap: TrapRequirement::PreservePrivatePreparationIdentity, cleanup, .. }
        if cleanup.iter().map(super::super::cleanup::CleanupEntry::owner_id).collect::<Vec<_>>() == [0]
    )));
    assert!(copied.steps().iter().any(|step| matches!(step,
        FlowStep::Call { entry: CallEntry::NonEmptyOwner(0), created_owners, .. } if created_owners.is_empty()
    )));
    assert!(copied.steps().iter().any(|step| matches!(step,
        FlowStep::ConfirmRelease { owner: 0, unresolved_on_fault, .. }
        if unresolved_on_fault.iter().map(super::super::cleanup::CleanupEntry::owner_id).collect::<Vec<_>>() == [0]
    )));
    assert!(
        matches!(copied.steps().last(), Some(FlowStep::Return { cleanup, .. }) if cleanup.is_empty())
    );
}

#[test]
fn native_c_body_v0_unreleasable_malformed_output_stays_unresolved_without_guessed_release() {
    let mut document = capture::document();
    let operation = document["operations"]
        .as_array_mut()
        .expect("operations")
        .iter_mut()
        .find(|operation| operation["symbol"] == "fixture_copy_bytes")
        .expect("byte creator");
    let resource = operation["resources"]
        .as_array_mut()
        .expect("resources")
        .iter_mut()
        .find(|resource| resource["access"] == "create")
        .expect("created byte policy");
    resource["releasableOnMalformed"] = false.into();
    let capture = capture::captured(
        &[("buffer", capture::BUFFER), ("handle", capture::HANDLE), ("scalar", capture::SCALAR)],
        false,
        document,
    );
    let bodies = verify_bodies(&capture.sources, &capture.declarations)
        .expect("false remains an explicit weaker promise");
    let copied = bodies
        .functions()
        .iter()
        .find(|function| function.name() == "copied")
        .expect("byte copy wrapper");
    assert!(copied.steps().iter().any(|step| matches!(step,
        FlowStep::Take { owner: 0, malformed_release_allowed: false, malformed_unresolved_owner: Some(0), cleanup, .. }
            if cleanup.iter().all(|entry| entry.owner_id() != 0)
    )), "malformed pointer is excluded from release actions and retained as unresolved");
}

#[test]
fn native_c_body_v0_mixed_copy_failure_preserves_reverse_prefix_and_release_failure_overrides_it() {
    let capture = capture::append_handle(
        r#"
function mixed(seed: i32, bytes: Vec<i32>): Vec<i32> {
  const h: FfiHandleOut = Ffi.outHandle("fixture-c-v0@0/fixture_handle");
  const opened: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_open", seed, h);
  if (opened !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_open", opened); }
  const handle: FfiHandle = Ffi.takeHandle(h);
  const loan: FfiBytes = Ffi.borrowBytes(bytes);
  const out: FfiBytesOut = Ffi.outBytes();
  const length: FfiCountOut = Ffi.outCount();
  const copied: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_copy_bytes", loan, Ffi.byteLength(loan), out, length);
  if (copied !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_copy_bytes", copied); }
  const foreign: FfiOwnedBytes = Ffi.takeBytes(out, length);
  const value: Vec<i32> = Ffi.copyBytes(foreign);
  Ffi.release("fixture-c-v0@0/fixture_release_bytes", foreign);
  return value;
}
"#,
    );
    let bodies = verify_bodies(&capture.sources, &capture.declarations).expect("mixed prefix");
    let function =
        bodies.functions().iter().find(|function| function.name() == "mixed").expect("mixed body");
    assert_eq!(function.owner_origins()[0].kind(), "fixture-c-v0@0/fixture_handle");
    assert_eq!(function.owner_origins()[1].kind(), "fixture-c-v0@0/owned_bytes");
    assert!(function.steps().iter().any(|step| matches!(step,
        FlowStep::StatusGuard { call: 1, cleanup, .. }
        if cleanup.iter().map(super::super::cleanup::CleanupEntry::owner_id).collect::<Vec<_>>() == [0]
    )));
    assert!(function.steps().iter().any(|step| matches!(step,
        FlowStep::Copy { owner: 1, cleanup, .. }
        if cleanup.iter().map(super::super::cleanup::CleanupEntry::owner_id).collect::<Vec<_>>() == [1, 0]
    )));
    assert!(function.steps().iter().any(|step| matches!(step,
        FlowStep::ConfirmRelease { owner: 1, fault_route: super::super::FailureRoute::ReleaseFailureOverridesUnresolved, unresolved_on_fault, .. }
        if unresolved_on_fault.iter().map(super::super::cleanup::CleanupEntry::owner_id).collect::<Vec<_>>() == [1, 0]
    )));
    assert!(matches!(function.steps().last(), Some(FlowStep::Return { cleanup, .. })
        if cleanup.iter().map(super::super::cleanup::CleanupEntry::owner_id).collect::<Vec<_>>() == [0]));
}
