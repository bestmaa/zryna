use super::super::super::{BoundaryDrop, FailureRoute, TrapRequirement};
use super::{
    BoundaryExitKind, BoundaryOwner, PrivateOrigin, PrivatePreparation, function, reference,
    reject, verify_candidate,
};
use zryna_ir::data_ownership_v1 as limits;
use zryna_ownership_runtime_abi::{LogicalOperation, RuntimeStatus};

#[test]
fn native_c_private_boundary_v0_foreign_origin_and_validation_promise_cannot_be_rewritten_in_cleanup()
 {
    for defect in 0..3 {
        reject(
            |candidate| {
                let drop = function(candidate, 2)
                    .steps
                    .iter_mut()
                    .flat_map(|step| &mut step.exits)
                    .flat_map(|exit| &mut exit.cleanup)
                    .find_map(|drop| match drop {
                        BoundaryDrop::Foreign(owner) => Some(owner),
                        BoundaryDrop::Private(_) => None,
                    })
                    .expect("conditional foreign owner");
                match defect {
                    0 => drop.call += 1,
                    1 => drop.primary_slot += 1,
                    _ => drop.releasable_on_malformed = !drop.releasable_on_malformed,
                }
            },
            "ZRYNA-C4105",
            "boundary-foreign-cleanup-projection",
        );
    }
}

#[test]
fn native_c_private_boundary_v0_global_64_slots_survive_65_live_and_sequential_source_plans() {
    use super::super::super::FlowStep;
    use std::fmt::Write;
    for (count, sequential) in [(64, false), (65, false), (65, true)] {
        let mut source = String::from("function privateLimit(): i32 {\n");
        for index in 0..count {
            writeln!(source,
                "const o{index}: FfiHandleOut = Ffi.outHandle(\"fixture-c-v0@0/fixture_handle\");\n\
                 const s{index}: i32 = Ffi.rawCall(\"fixture-c-v0@0/fixture_open\", 0, o{index});\n\
                 if (s{index} !== 0) {{ return Ffi.foreignError(\"fixture-c-v0@0/fixture_open\", s{index}); }}"
            ).expect("independent bounded source");
            if sequential {
                writeln!(
                    source,
                    "const h{index}: FfiHandle = Ffi.takeHandle(o{index});\n\
                    Ffi.release(\"fixture-c-v0@0/fixture_close\", h{index});"
                )
                .expect("independent confirmed releases");
            }
        }
        source.push_str("return 0; }");
        let capture = super::capture::compact(&source);
        let bodies = super::verify_bodies(&capture.sources, &capture.declarations)
            .expect("global reservation plans");
        let boundary = super::compose_private_boundaries(&capture.sources, &bodies)
            .expect("bounded private composition");
        let original = bodies
            .functions()
            .iter()
            .find(|function| function.name() == "privateLimit")
            .expect("source function");
        let function = super::named(&boundary, "privateLimit");
        assert!(function.private_owners.is_empty());
        assert_eq!(original.owner_origins().len(), count);
        let mut acquisitions = 0;
        for step in original.steps() {
            if let FlowStep::Reserve { live_limit, maximum_new_owners, cleanup, .. } = step {
                assert_eq!(*live_limit, 64);
                assert!(*maximum_new_owners <= 1);
                if *maximum_new_owners == 1 {
                    acquisitions += 1;
                    if sequential {
                        assert!(cleanup.is_empty());
                    }
                }
            }
        }
        assert_eq!(acquisitions, count);
        assert_eq!(
            function.steps.last().expect("return").exits[0].cleanup.len(),
            if sequential { 0 } else { count }
        );
    }
}

#[test]
fn native_c_private_boundary_v0_missing_function_expression_step_and_preparation_fail_atomic_replay()
 {
    reject(
        |candidate| {
            candidate.functions.pop();
        },
        "ZRYNA-C4106",
        "boundary-complete-function-inventory",
    );
    reject(
        |candidate| {
            function(candidate, 0).expression_origins.pop();
        },
        "ZRYNA-C4106",
        "boundary-original-function-occupants",
    );
    reject(
        |candidate| {
            function(candidate, 0).steps.pop();
        },
        "ZRYNA-C4106",
        "boundary-original-function-occupants",
    );
    reject(
        |candidate| {
            function(candidate, 0).steps[0].source_step = 1;
        },
        "ZRYNA-C4106",
        "boundary-complete-step-order",
    );
    reject(
        |candidate| {
            function(candidate, 0).steps[0].preparation = None;
        },
        "ZRYNA-C4106",
        "boundary-private-preparation-inventory",
    );
}

#[test]
fn native_c_private_boundary_v0_genuine_wrong_operation_status_and_foreign_trap_cannot_replace_private_fault()
 {
    reject(
        |candidate| {
            let release = candidate
                .layouts
                .runtime
                .operations()
                .find(|operation| operation.operation() == LogicalOperation::StringRelease)
                .expect("genuine release")
                .id();
            let loan = function(candidate, 0)
                .steps
                .iter_mut()
                .find_map(|step| match &mut step.preparation {
                    Some(PrivatePreparation::Loan(loan)) => Some(loan),
                    _ => None,
                })
                .expect("loan");
            loan.faults[0].operation = release;
        },
        "ZRYNA-C4105",
        "boundary-byte-packing",
    );
    reject(
        |candidate| {
            let host = candidate
                .layouts
                .runtime
                .status_declarations()
                .find(|declaration| declaration.status() == RuntimeStatus::AbiViolation)
                .expect("issued host failure");
            let copy = function(candidate, 2)
                .steps
                .iter_mut()
                .find_map(|step| match &mut step.preparation {
                    Some(PrivatePreparation::Copy(copy)) => Some(copy),
                    _ => None,
                })
                .expect("copy");
            copy.faults[0].declaration = host;
        },
        "ZRYNA-C4105",
        "boundary-copy-private-issuer",
    );
    reject(
        |candidate| {
            let exit = function(candidate, 0)
                .steps
                .iter_mut()
                .flat_map(|step| &mut step.exits)
                .find(|exit| matches!(exit.kind, BoundaryExitKind::PrivateTrap(_)))
                .expect("private fault");
            exit.kind = BoundaryExitKind::ForeignTrap(TrapRequirement::ForeignLength);
        },
        "ZRYNA-C4105",
        "boundary-exit-domain",
    );
}

#[test]
fn native_c_private_boundary_v0_wrong_completion_source_and_private_release_issuer_reject() {
    reject(
        |candidate| {
            let step = function(candidate, 2)
                .steps
                .iter_mut()
                .find(|step| matches!(step.preparation, Some(PrivatePreparation::Copy(_))))
                .expect("copy");
            step.completed[0] = BoundaryOwner::Private(PrivateOrigin::Parameter(0));
        },
        "ZRYNA-C4105",
        "boundary-completion-origin",
    );
    reject(
        |candidate| {
            function(candidate, 0).expression_origins[0] = Some(PrivateOrigin::Copy(0));
        },
        "ZRYNA-C4106",
        "boundary-original-function-occupants",
    );
    reject(
        |candidate| {
            let wrong = candidate
                .layouts
                .runtime
                .operations()
                .find(|operation| operation.operation() == LogicalOperation::StringRelease)
                .expect("issued other release")
                .id();
            let drop = function(candidate, 0).steps.last_mut().expect("return").exits[0]
                .cleanup
                .iter_mut()
                .find_map(|drop| match drop {
                    BoundaryDrop::Private(owner) => Some(owner),
                    BoundaryDrop::Foreign(_) => None,
                })
                .expect("private drop");
            drop.release = wrong;
        },
        "ZRYNA-C4105",
        "boundary-private-release-issuer",
    );
}

#[test]
fn native_c_private_boundary_v0_cleanup_loan_end_and_protected_return_cannot_be_omitted_or_reordered()
 {
    reject(
        |candidate| {
            function(candidate, 0).steps.last_mut().expect("return").exits[0].cleanup.swap(0, 1);
        },
        "ZRYNA-C4105",
        "boundary-reverse-completion-order",
    );
    reject(
        |candidate| {
            function(candidate, 0).steps.last_mut().expect("return").exits[0].end_loans.clear();
        },
        "ZRYNA-C4105",
        "boundary-loan-end-order",
    );
    reject(
        |candidate| {
            function(candidate, 2).steps.last_mut().expect("return").exits[0].protected_result =
                None;
        },
        "ZRYNA-C4105",
        "boundary-exit-domain",
    );
    reject(
        |candidate| {
            function(candidate, 0).steps.last_mut().expect("return").exits[0].cleanup.pop();
        },
        "ZRYNA-C4105",
        "boundary-reverse-completion-order",
    );
}

#[test]
fn native_c_private_boundary_v0_release_retry_and_process_cleanup_guarantee_are_rejected() {
    reject(
        |candidate| {
            let copy_cleanup = function(candidate, 2)
                .steps
                .iter()
                .flat_map(|step| &step.exits)
                .find(|exit| matches!(exit.kind, BoundaryExitKind::PrivateTrap(_)))
                .expect("cleanup")
                .cleanup
                .clone();
            let release = function(candidate, 2)
                .steps
                .iter_mut()
                .flat_map(|step| &mut step.exits)
                .find(|exit| {
                    exit.kind
                        == BoundaryExitKind::ForeignFailure(
                            FailureRoute::ReleaseFailureOverridesUnresolved,
                        )
                })
                .expect("release fault");
            release.cleanup = copy_cleanup;
        },
        "ZRYNA-C4105",
        "boundary-unresolved-no-retry",
    );
    reject(
        |candidate| {
            let process = function(candidate, 0)
                .steps
                .iter_mut()
                .flat_map(|step| &mut step.exits)
                .find(|exit| {
                    exit.kind
                        == BoundaryExitKind::ForeignFailure(
                            FailureRoute::ProcessFailureNoCleanupGuarantee,
                        )
                })
                .expect("process failure");
            process.cleanup_required = true;
        },
        "ZRYNA-C4105",
        "boundary-exit-domain",
    );
}

#[test]
fn native_c_private_boundary_v0_checked_budget_boundaries_and_overflow_precede_candidate_sealing() {
    use super::super::super::private_boundary::checked_count;
    for maximum in [
        limits::MAX_VALUES_PER_FUNCTION,
        limits::MAX_VALUES_PER_PROGRAM,
        limits::MAX_PLACES_PER_FUNCTION,
        limits::MAX_OWNERSHIP_TRANSITIONS_PER_FUNCTION,
        limits::MAX_ACTIVE_BORROWS_PER_FUNCTION,
        limits::MAX_DROP_ACTIONS_PER_FUNCTION,
        limits::MAX_CLEANUP_PLANS_PER_FUNCTION,
    ] {
        assert_eq!(checked_count(maximum - 1, 1, maximum).expect("exact maximum"), maximum);
        assert_eq!(
            checked_count(maximum, 1, maximum).expect_err("first excess").code(),
            "ZRYNA-C4107"
        );
    }
    assert_eq!(
        checked_count(usize::MAX, 1, usize::MAX).expect_err("overflow").code(),
        "ZRYNA-C4107"
    );
    let (capture, bodies, mut candidate) = reference();
    function(&mut candidate, 0)
        .expression_origins
        .resize(limits::MAX_VALUES_PER_FUNCTION + 1, None);
    assert_eq!(
        verify_candidate(&capture.sources, &bodies, candidate)
            .expect_err("candidate excess")
            .code(),
        "ZRYNA-C4107"
    );
}
