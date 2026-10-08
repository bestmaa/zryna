//! Conditional foreign and private exit domains, in source diagnostic order.

use super::super::super::{CleanupEntry, FailureRoute, FlowStep, TrapRequirement};
use super::super::{BoundaryExitKind, BoundaryOwner, PrivatePreparation};

pub(in crate::native_c_v0::body::private_boundary) struct ExitRule<'a> {
    pub(super) kind: BoundaryExitKind,
    pub(super) foreign: &'a [CleanupEntry],
    pub(super) unresolved: Vec<BoundaryOwner>,
    pub(super) cleanup_required: bool,
    pub(super) stop: bool,
    pub(super) nonempty: bool,
}
impl<'a> ExitRule<'a> {
    fn new(kind: BoundaryExitKind, foreign: &'a [CleanupEntry]) -> Self {
        Self {
            kind,
            foreign,
            unresolved: Vec::new(),
            cleanup_required: true,
            stop: false,
            nonempty: false,
        }
    }
}

pub(in crate::native_c_v0::body::private_boundary) fn rules<'a>(
    step: &'a FlowStep,
    preparation: Option<&PrivatePreparation>,
    release_call: bool,
) -> Vec<ExitRule<'a>> {
    let mut result = Vec::new();
    match step {
        FlowStep::PrepareLoan { traps, cleanup, .. } => {
            for trap in traps {
                if *trap != TrapRequirement::PreservePrivatePreparationIdentity {
                    result.push(ExitRule::new(BoundaryExitKind::ForeignTrap(*trap), cleanup));
                }
            }
        }
        FlowStep::Reserve { trap, cleanup, .. } => {
            result.push(ExitRule::new(BoundaryExitKind::ForeignTrap(*trap), cleanup));
        }
        FlowStep::Call {
            unknown_status_route,
            unknown_status_unresolved_owners,
            process_fault_route,
            created_owners,
            cleanup,
            boundary_checks,
            ..
        } => {
            for check in boundary_checks {
                result.push(ExitRule::new(
                    BoundaryExitKind::ForeignBoundaryFailure(check.clone()),
                    cleanup,
                ));
            }
            let mut unknown =
                ExitRule::new(BoundaryExitKind::ForeignFailure(*unknown_status_route), cleanup);
            unknown.unresolved = unknown_status_unresolved_owners
                .iter()
                .copied()
                .map(BoundaryOwner::Foreign)
                .collect();
            unknown.stop = release_call;
            result.push(unknown);
            let mut process =
                ExitRule::new(BoundaryExitKind::ForeignFailure(*process_fault_route), &[]);
            process.cleanup_required = false;
            process.unresolved =
                created_owners.iter().copied().map(BoundaryOwner::Foreign).collect();
            result.push(process);
        }
        FlowStep::StatusGuard { recoverable_route, cleanup, .. } => result
            .push(ExitRule::new(BoundaryExitKind::ForeignFailure(*recoverable_route), cleanup)),
        FlowStep::Take { malformed_route, malformed_unresolved_owner, cleanup, .. } => {
            let mut malformed =
                ExitRule::new(BoundaryExitKind::ForeignFailure(*malformed_route), cleanup);
            malformed.unresolved =
                malformed_unresolved_owner.iter().copied().map(BoundaryOwner::Foreign).collect();
            result.push(malformed);
        }
        FlowStep::ConfirmRelease { fault_route, .. } => {
            let mut failure = ExitRule::new(BoundaryExitKind::ForeignFailure(*fault_route), &[]);
            failure.stop = true;
            result.push(failure);
        }
        FlowStep::Return { cleanup, .. } => {
            result.push(ExitRule::new(BoundaryExitKind::Return, cleanup));
        }
        _ => {}
    }
    let prepared = match preparation {
        Some(PrivatePreparation::Loan(loan)) => {
            loan.allocation.map(|allocation| (allocation, &loan.faults, loan.scratch))
        }
        Some(PrivatePreparation::Copy(copy)) => {
            Some((copy.allocation, &copy.faults, Some(copy.result)))
        }
        None => None,
    };
    if let Some((allocation, faults, partial_owner)) = prepared {
        let cleanup = match step {
            FlowStep::PrepareLoan { cleanup, .. } | FlowStep::Copy { cleanup, .. } => {
                cleanup.as_slice()
            }
            _ => &[],
        };
        for kind in faults
            .iter()
            .copied()
            .map(BoundaryExitKind::PrivateTrap)
            .chain(std::iter::once(BoundaryExitKind::PrivateAbiFailure(allocation)))
        {
            let mut failure = ExitRule::new(kind, cleanup);
            failure.nonempty = true;
            result.push(failure);
        }
        let mut process = ExitRule::new(
            BoundaryExitKind::ForeignFailure(FailureRoute::ProcessFailureNoCleanupGuarantee),
            &[],
        );
        process.cleanup_required = false;
        process.nonempty = true;
        process.unresolved = partial_owner.into_iter().map(BoundaryOwner::Private).collect();
        result.push(process);
    }
    result
}
