use super::super::super::{BoundaryDrop, FailureRoute, TrapRequirement};
use super::{
    BoundaryExitKind, BoundaryOwner, Candidate, PrivateOrigin, PrivatePreparation,
    VerifiedForeignBodies, capture, compose_private_boundaries, reference, verify_bodies,
    verify_candidate,
};
use zryna_ownership_runtime_abi::{
    RuntimeStatus, VerifiedStatusDisposition, VerifiedStatusTrapIdentity,
};

pub(super) fn mixed() -> (capture::Capture, VerifiedForeignBodies, Candidate) {
    let extra = r#"
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
"#;
    let capture = capture::append_handle(extra);
    let bodies =
        verify_bodies(&capture.sources, &capture.declarations).expect("genuine mixed source");
    let candidate = super::build_candidate(&capture.sources, &bodies).expect("mixed candidate");
    (capture, bodies, candidate)
}

#[test]
fn native_c_private_boundary_v0_private_allocation_capacity_issuers_stay_separate_from_foreign_status_one()
 {
    let (capture, bodies, _) = reference();
    let boundary =
        compose_private_boundaries(&capture.sources, &bodies).expect("issued private faults");
    for function in boundary.functions() {
        for step in &function.steps {
            let faults = match &step.preparation {
                Some(PrivatePreparation::Loan(loan)) => &loan.faults,
                Some(PrivatePreparation::Copy(copy)) => &copy.faults,
                None => continue,
            };
            for (index, fault) in faults.iter().enumerate() {
                assert_eq!(fault.runtime, boundary.runtime_abi().identity());
                assert_eq!(
                    fault.declaration.disposition(),
                    VerifiedStatusDisposition::ControlledTrap
                );
                assert_eq!(
                    (fault.declaration.status(), fault.declaration.trap_identity()),
                    if index == 0 {
                        (RuntimeStatus::Allocation, Some(VerifiedStatusTrapIdentity::AllocationV1))
                    } else {
                        (RuntimeStatus::Capacity, Some(VerifiedStatusTrapIdentity::CapacityV1))
                    }
                );
                assert!(
                    step.exits
                        .iter()
                        .any(|exit| exit.kind == BoundaryExitKind::PrivateTrap(*fault))
                );
            }
        }
    }
    assert!(
        boundary.functions()[0].steps.iter().flat_map(|step| &step.exits).any(|exit| exit.kind
            == BoundaryExitKind::ForeignFailure(FailureRoute::DeclaredForeignError))
    );
}

#[test]
fn native_c_private_boundary_v0_mixed_copy_failure_reverses_private_and_foreign_completion_without_result()
 {
    let (capture, bodies, candidate) = mixed();
    let boundary =
        verify_candidate(&capture.sources, &bodies, candidate).expect("independent replay");
    let function = super::named(&boundary, "mixed");
    let loan = function
        .steps
        .iter()
        .find_map(|step| match &step.preparation {
            Some(PrivatePreparation::Loan(loan)) => Some(loan),
            _ => None,
        })
        .expect("scratch");
    let copy_step = function
        .steps
        .iter()
        .find(|step| matches!(step.preparation, Some(PrivatePreparation::Copy(_))))
        .expect("copy preparation");
    let failure = copy_step
        .exits
        .iter()
        .find(|exit| matches!(exit.kind, BoundaryExitKind::PrivateTrap(_)))
        .expect("private fault");
    assert_eq!(
        failure.cleanup.iter().map(BoundaryDrop::owner).collect::<Vec<_>>(),
        [
            BoundaryOwner::Foreign(1),
            BoundaryOwner::Private(PrivateOrigin::Packed(loan.expression)),
            BoundaryOwner::Foreign(0),
            BoundaryOwner::Private(PrivateOrigin::Parameter(1))
        ]
    );
    assert_eq!(failure.end_loans, [loan.token]);
    assert!(failure.protected_result.is_none());
    assert!(
        !failure
            .cleanup
            .iter()
            .any(|drop| matches!(drop.owner(), BoundaryOwner::Private(PrivateOrigin::Copy(_))))
    );
    assert_eq!(
        failure
            .cleanup
            .iter()
            .filter_map(|drop| match drop {
                BoundaryDrop::Foreign(entry) => Some(entry.owner_id()),
                BoundaryDrop::Private(_) => None,
            })
            .collect::<Vec<_>>(),
        [1, 0]
    );
}

#[test]
fn native_c_private_boundary_v0_return_transfer_follows_cleanup_and_release_failure_keeps_pending_result()
 {
    let (capture, bodies, candidate) = mixed();
    let boundary = verify_candidate(&capture.sources, &bodies, candidate).expect("mixed return");
    let function = super::named(&boundary, "mixed");
    let returned = function.steps.last().expect("return").exits.last().expect("return exit");
    let result = returned.protected_result.expect("protected private result");
    assert!(matches!(result, PrivateOrigin::Copy(_)));
    assert_eq!(
        returned.cleanup.iter().map(BoundaryDrop::owner).collect::<Vec<_>>()[1..],
        [BoundaryOwner::Foreign(0), BoundaryOwner::Private(PrivateOrigin::Parameter(1))]
    );
    assert_eq!(
        returned.unresolved_after_release_failure(1).expect("H release failure"),
        [
            BoundaryOwner::Foreign(0),
            BoundaryOwner::Private(PrivateOrigin::Parameter(1)),
            BoundaryOwner::Private(result)
        ]
    );
    assert!(returned.cleanup.iter().all(|drop| drop.owner() != BoundaryOwner::Private(result)));
    assert!(returned.unresolved_after_release_failure(returned.cleanup.len() + 1).is_none());
    assert!(returned.unresolved_after_release_failure(returned.cleanup.len()).is_none());
    assert_eq!(returned.release_failure_route, FailureRoute::ReleaseFailureOverridesUnresolved);
    let release_fault = function
        .steps
        .iter()
        .flat_map(|step| &step.exits)
        .find(|exit| {
            exit.kind
                == BoundaryExitKind::ForeignFailure(FailureRoute::ReleaseFailureOverridesUnresolved)
        })
        .expect("explicit F release failure");
    assert!(release_fault.cleanup.is_empty());
    assert_eq!(release_fault.unresolved[0], BoundaryOwner::Private(result));
    assert!(release_fault.unresolved.contains(&BoundaryOwner::Foreign(1)));
}

#[test]
fn native_c_private_boundary_v0_malformed_without_release_promise_preserves_private_cleanup_and_unresolved_foreign_owner()
 {
    let mut document = capture::document();
    let creator = document["operations"]
        .as_array_mut()
        .expect("operations")
        .iter_mut()
        .find(|operation| operation["symbol"] == "fixture_copy_bytes")
        .expect("creator");
    let resource = creator["resources"]
        .as_array_mut()
        .expect("resources")
        .iter_mut()
        .find(|resource| resource["access"] == "create")
        .expect("created owner");
    resource["releasableOnMalformed"] = false.into();
    let capture = capture::captured(
        &[("buffer", capture::BUFFER), ("handle", capture::HANDLE), ("scalar", capture::SCALAR)],
        false,
        document,
    );
    let bodies =
        verify_bodies(&capture.sources, &capture.declarations).expect("captured weaker promise");
    let boundary = compose_private_boundaries(&capture.sources, &bodies).expect("malformed route");
    let function = super::named(&boundary, "copied");
    let malformed = function
        .steps
        .iter()
        .zip(bodies.functions()[2].steps())
        .find_map(|(step, original)| {
            if matches!(original, super::super::super::FlowStep::Take { .. }) {
                step.exits.first()
            } else {
                None
            }
        })
        .expect("take metadata failure");
    assert_eq!(malformed.unresolved, [BoundaryOwner::Foreign(0)]);
    assert_eq!(malformed.cleanup.len(), 2);
    assert!(malformed.cleanup.iter().all(|drop| matches!(drop, BoundaryDrop::Private(_))));
}

#[test]
fn native_c_private_boundary_v0_release_failure_overrides_controlled_trap_and_process_failure_promises_no_cleanup()
 {
    let (capture, bodies, candidate) = mixed();
    let boundary = verify_candidate(&capture.sources, &bodies, candidate).expect("fault routes");
    let function = super::named(&boundary, "mixed");
    let trap = function
        .steps
        .iter()
        .flat_map(|step| &step.exits)
        .find(|exit| {
            matches!(exit.kind, BoundaryExitKind::PrivateTrap(_)) && exit.cleanup.len() == 4
        })
        .expect("copy failure cleanup");
    assert_eq!(
        trap.unresolved_after_release_failure(2).expect("H drop fault"),
        [BoundaryOwner::Foreign(0), BoundaryOwner::Private(PrivateOrigin::Parameter(1))]
    );
    for exit in function.steps.iter().flat_map(|step| &step.exits).filter(|exit| {
        exit.kind
            == BoundaryExitKind::ForeignFailure(FailureRoute::ProcessFailureNoCleanupGuarantee)
    }) {
        assert!(!exit.cleanup_required && exit.cleanup.is_empty() && exit.end_loans.is_empty());
        assert!(exit.protected_result.is_none());
    }
    assert!(
        function.steps.iter().flat_map(|step| &step.exits).any(|exit| exit.kind
            == BoundaryExitKind::ForeignTrap(TrapRequirement::ForeignResourceLimit))
    );
    let copy_step = function
        .steps
        .iter()
        .find(|step| matches!(step.preparation, Some(PrivatePreparation::Copy(_))))
        .expect("copy requirements");
    let partial = copy_step
        .exits
        .iter()
        .find(|exit| !exit.cleanup_required)
        .expect("uncommitted process fault");
    assert!(
        partial
            .unresolved
            .iter()
            .any(|owner| matches!(owner, BoundaryOwner::Private(PrivateOrigin::Copy(_))))
    );
}

#[test]
fn native_c_private_boundary_v0_untaken_outputs_and_unknown_status_keep_exact_conditional_obligations()
 {
    let capture = capture::append_handle(
        r#"
function untaken(bytes: Vec<i32>): i32 {
  const one: FfiHandleOut = Ffi.outHandle("fixture-c-v0@0/fixture_handle");
  const a: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_open", 1, one);
  const two: FfiHandleOut = Ffi.outHandle("fixture-c-v0@0/fixture_handle");
  const b: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_open", 2, two);
  if (b !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_open", b); }
  if (a !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_open", a); }
  return 0;
}
"#,
    );
    let bodies = verify_bodies(&capture.sources, &capture.declarations).expect("untaken body");
    let boundary = compose_private_boundaries(&capture.sources, &bodies).expect("combined cleanup");
    let function = super::named(&boundary, "untaken");
    let returned = &function.steps.last().expect("return").exits[0];
    assert_eq!(
        returned.cleanup.iter().map(BoundaryDrop::owner).collect::<Vec<_>>(),
        [
            BoundaryOwner::Foreign(1),
            BoundaryOwner::Foreign(0),
            BoundaryOwner::Private(PrivateOrigin::Parameter(0))
        ]
    );
    assert!(returned.cleanup.iter().all(|drop| match drop {
        BoundaryDrop::Foreign(entry) => entry.validation_required(),
        BoundaryDrop::Private(_) => true,
    }));
    let unknown = function
        .steps
        .iter()
        .flat_map(|step| &step.exits)
        .filter(|exit| exit.kind == BoundaryExitKind::ForeignFailure(FailureRoute::HostAbiFailure))
        .collect::<Vec<_>>();
    assert_eq!(unknown[1].unresolved, [BoundaryOwner::Foreign(1)]);
    assert_eq!(
        unknown[1].cleanup.iter().map(BoundaryDrop::owner).collect::<Vec<_>>(),
        [BoundaryOwner::Foreign(0), BoundaryOwner::Private(PrivateOrigin::Parameter(0))]
    );
    let declared = function
        .steps
        .iter()
        .flat_map(|step| &step.exits)
        .filter(|exit| {
            exit.kind == BoundaryExitKind::ForeignFailure(FailureRoute::DeclaredForeignError)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        declared[0].cleanup.iter().map(BoundaryDrop::owner).collect::<Vec<_>>(),
        [BoundaryOwner::Foreign(0), BoundaryOwner::Private(PrivateOrigin::Parameter(0))]
    );
}
