//! Independent successful-completion ledger, loan-end order and terminal cleanup replay.

use crate::{IrError, raw, require};
use zryna_semantics::native_c_v0::body::{
    BoundaryDrop, BoundaryExitKind, BoundaryOwner, FailureRoute, FlowStep, FunctionBoundary,
    PrivateOrigin,
};

pub(super) fn check(function: &raw::Function, authority: &FunctionBoundary) -> Result<(), IrError> {
    let mut completed = function
        .private_owners
        .iter()
        .filter_map(|owner| {
            matches!(owner.origin, PrivateOrigin::Parameter(_))
                .then_some(BoundaryOwner::Private(owner.origin))
        })
        .collect::<Vec<_>>();
    let mut loans = Vec::new();
    for (index, (effect, sealed)) in function.effects.iter().zip(&authority.steps).enumerate() {
        require(effect.exits == sealed.exits, "ZRYNA-C4105", "ir-complete-authenticated-exits")?;
        let commitments = match &effect.operation {
            FlowStep::PrepareLoan { expression, utf8: false, .. } => {
                vec![BoundaryOwner::Private(PrivateOrigin::Packed(*expression))]
            }
            FlowStep::Copy { expression, .. } => {
                vec![BoundaryOwner::Private(PrivateOrigin::Copy(*expression))]
            }
            FlowStep::Call { created_owners, .. } => {
                created_owners.iter().copied().map(BoundaryOwner::Foreign).collect()
            }
            _ => Vec::new(),
        };
        require(
            effect.completed == commitments,
            "ZRYNA-C4105",
            "ir-conditional-completion-origin",
        )?;
        for exit in &effect.exits {
            terminal_exit(function, effect, sealed, index, exit, &completed, &loans)?;
        }
        for owner in commitments {
            require(!completed.contains(&owner), "ZRYNA-C4105", "ir-duplicate-owner-completion")?;
            completed.push(owner);
        }
        match &effect.operation {
            FlowStep::PrepareLoan { token, .. } => {
                require(!loans.contains(token), "ZRYNA-C4105", "ir-duplicate-live-loan")?;
                loans.push(*token);
            }
            FlowStep::ConfirmRelease { owner, .. } => {
                let at = completed
                    .iter()
                    .position(|entry| *entry == BoundaryOwner::Foreign(*owner))
                    .ok_or_else(|| IrError::new("ZRYNA-C4105", "ir-release-live-owner"))?;
                completed.remove(at);
            }
            _ => {}
        }
    }
    Ok(())
}

fn terminal_exit(
    function: &raw::Function,
    effect: &raw::Effect,
    sealed: &zryna_semantics::native_c_v0::body::BoundaryStep,
    index: usize,
    exit: &zryna_semantics::native_c_v0::body::BoundaryExit,
    completed: &[BoundaryOwner],
    loans: &[usize],
) -> Result<(), IrError> {
    require(
        exit.release_failure_route == FailureRoute::ReleaseFailureOverridesUnresolved,
        "ZRYNA-C4105",
        "ir-release-failure-overrides",
    )?;
    let protected = if let FlowStep::Return { expression, .. } = &effect.operation {
        function.values.get(*expression).and_then(|value| value.origin)
    } else {
        None
    };
    require(
        exit.protected_result
            == if exit.kind == BoundaryExitKind::Return { protected } else { None },
        "ZRYNA-C4105",
        "ir-protected-result-transfer",
    )?;
    let process = matches!(
        exit.kind,
        BoundaryExitKind::ForeignFailure(FailureRoute::ProcessFailureNoCleanupGuarantee)
    );
    require(exit.cleanup_required != process, "ZRYNA-C4105", "ir-process-cleanup-domain")?;
    let expected_loans =
        if process { Vec::new() } else { loans.iter().rev().copied().collect::<Vec<_>>() };
    require(exit.end_loans == expected_loans, "ZRYNA-C4105", "ir-loans-end-before-drops")?;
    let release_call = matches!((&effect.operation, function.effects.get(index + 1).map(|e| &e.operation)),
        (FlowStep::Call { call, .. }, Some(FlowStep::ConfirmRelease { call: confirmed, .. })) if call == confirmed);
    let stop = matches!(
        exit.kind,
        BoundaryExitKind::ForeignFailure(FailureRoute::ReleaseFailureOverridesUnresolved)
    ) || release_call
        && matches!(exit.kind, BoundaryExitKind::ForeignFailure(FailureRoute::HostAbiFailure));
    if process || stop {
        require(
            exit.cleanup.is_empty()
                && exit
                    .unresolved
                    .starts_with(&completed.iter().rev().copied().collect::<Vec<_>>()),
            "ZRYNA-C4105",
            "ir-no-guessed-cleanup-or-retry",
        )?;
        return Ok(());
    }
    // Foreign eligibility and malformed-release promises come from the actual issuer's
    // exit, never a caller-selected filter. Private owners come from independent replay.
    let allowed = &sealed
        .exits
        .iter()
        .find(|expected| expected.kind == exit.kind)
        .ok_or_else(|| IrError::new("ZRYNA-C4105", "ir-exit-domain"))?
        .cleanup;
    let expected_order = completed
        .iter()
        .rev()
        .copied()
        .filter(|owner| match owner {
            BoundaryOwner::Private(origin) => Some(*origin) != exit.protected_result,
            BoundaryOwner::Foreign(_) => allowed.iter().any(|drop| drop.owner() == *owner),
        })
        .collect::<Vec<_>>();
    require(
        exit.cleanup.iter().map(BoundaryDrop::owner).eq(expected_order),
        "ZRYNA-C4105",
        "ir-reverse-completion-cleanup",
    )?;
    for drop in &exit.cleanup {
        if let BoundaryDrop::Private(owner) = drop {
            require(
                function.private_owners.iter().find(|entry| entry.origin == owner.origin)
                    == Some(owner),
                "ZRYNA-C4105",
                "ir-exact-private-drop-issuer",
            )?;
        }
    }
    Ok(())
}
