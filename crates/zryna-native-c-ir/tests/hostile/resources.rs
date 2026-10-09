//! Independent resource preparation, storage and terminal-cleanup mutations.

use super::reject;
use zryna_semantics::native_c_v0::body::{
    BoundaryExitKind, FailureRoute, FlowStep, PrivatePreparation,
};

#[test]
fn reservation_status_guard_initialized_read_and_confirmed_release_are_exact() {
    for defect in 0..4 {
        reject(
            |p| {
                for effect in p.functions.iter_mut().flat_map(|f| &mut f.effects) {
                    match (&mut effect.operation, defect) {
                        (FlowStep::Reserve { maximum_new_owners, .. }, 0) => {
                            *maximum_new_owners += 1;
                            break;
                        }
                        (FlowStep::StatusGuard { call, .. }, 1)
                        | (FlowStep::ReadOutput { call, .. }, 2) => {
                            *call = 99;
                            break;
                        }
                        (FlowStep::ConfirmRelease { owner, .. }, 3) => {
                            *owner = 99;
                            break;
                        }
                        _ => {}
                    }
                }
            },
            "ZRYNA-C4105",
        );
    }
}

#[test]
fn packing_cast_commit_order_and_omitted_checks_reject_independently() {
    for defect in 0..4 {
        reject(
            |p| {
                let loan = p.functions[0]
                    .effects
                    .iter_mut()
                    .find_map(|e| match &mut e.preparation {
                        Some(PrivatePreparation::Loan(l)) => Some(l),
                        _ => None,
                    })
                    .expect("packing");
                match defect {
                    0 => loan.source_stride = 1,
                    1 => loan.backing_stride = 4,
                    2 => loan.stages.swap(3, 4),
                    _ => {
                        loan.stages.remove(1);
                    }
                }
            },
            "ZRYNA-C4105",
        );
    }
}

#[test]
fn private_copy_issuer_fault_domain_and_complete_initialization_cannot_be_forged() {
    for defect in 0..3 {
        reject(
            |p| {
                let wrong_issuer = p.functions[0].private_owners[0].release;
                let copy = p.functions[2]
                    .effects
                    .iter_mut()
                    .find_map(|e| match &mut e.preparation {
                        Some(PrivatePreparation::Copy(c)) => Some(c),
                        _ => None,
                    })
                    .expect("copy");
                match defect {
                    0 => copy.zero_extend_bytes = false,
                    1 => copy.faults[0].operation = wrong_issuer,
                    _ => copy.stages.swap(0, 4),
                }
            },
            "ZRYNA-C4105",
        );
    }
    reject(
        |p| {
            let exit = p
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.effects)
                .flat_map(|e| &mut e.exits)
                .find(|e| matches!(e.kind, BoundaryExitKind::PrivateTrap(_)))
                .expect("private trap");
            exit.kind = BoundaryExitKind::ForeignFailure(FailureRoute::DeclaredForeignError);
        },
        "ZRYNA-C4105",
    );
}

#[test]
fn loan_end_and_mixed_reverse_cleanup_cannot_be_omitted_reordered_or_duplicated() {
    for defect in 0..4 {
        reject(
            |p| {
                let exits =
                    p.functions.iter_mut().flat_map(|f| &mut f.effects).flat_map(|e| &mut e.exits);
                if defect == 0 {
                    exits
                        .into_iter()
                        .find(|e| !e.end_loans.is_empty())
                        .expect("loan end")
                        .end_loans
                        .clear();
                } else {
                    let exit =
                        exits.into_iter().find(|e| e.cleanup.len() > 1).expect("mixed cleanup");
                    match defect {
                        1 => {
                            exit.cleanup.pop();
                        }
                        2 => exit.cleanup.swap(0, 1),
                        _ => exit.cleanup.push(exit.cleanup[0].clone()),
                    }
                }
            },
            "ZRYNA-C4105",
        );
    }
}

#[test]
fn pending_return_release_override_and_process_unresolved_obligations_are_preserved() {
    reject(
        |p| {
            p.functions[2]
                .effects
                .iter_mut()
                .flat_map(|e| &mut e.exits)
                .find(|e| e.kind == BoundaryExitKind::Return)
                .expect("owned return")
                .protected_result = None;
        },
        "ZRYNA-C4105",
    );
    reject(
        |p| {
            p.functions[0]
                .effects
                .iter_mut()
                .flat_map(|e| &mut e.exits)
                .find(|e| !e.cleanup.is_empty())
                .expect("cleanup")
                .release_failure_route = FailureRoute::DeclaredForeignError;
        },
        "ZRYNA-C4105",
    );
    reject(
        |p| {
            p.functions[0]
                .effects
                .iter_mut()
                .flat_map(|e| &mut e.exits)
                .find(|e| !e.cleanup_required)
                .expect("process edge")
                .cleanup_required = true;
        },
        "ZRYNA-C4105",
    );
    reject(
        |p| {
            p.functions[2]
                .effects
                .iter_mut()
                .flat_map(|e| &mut e.exits)
                .find(|e| !e.unresolved.is_empty())
                .expect("unresolved owner")
                .unresolved
                .clear();
        },
        "ZRYNA-C4105",
    );
}

#[test]
fn stale_layout_and_missing_storage_descriptor_cannot_mint_an_ir_seal() {
    reject(
        |p| {
            p.storage.native[0] ^= 1;
        },
        "ZRYNA-C4102",
    );
    reject(
        |p| {
            p.functions[0].parameter_layouts.clear();
        },
        "ZRYNA-C4105",
    );
}
