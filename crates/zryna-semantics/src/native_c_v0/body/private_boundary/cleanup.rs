//! Unified conditional completion order, protected return and release-failure override.

use super::super::{CleanupEntry, FailureRoute, FlowStep};
use super::owners::SourceOwners;
use super::{
    BoundaryDrop, BoundaryError, BoundaryExit, BoundaryExitKind, BoundaryOwner, PrivateOrigin,
};

mod exit_rules;
pub(super) use exit_rules::{ExitRule, rules};

pub(super) struct Registry {
    pub(super) completed: Vec<BoundaryOwner>,
    pub(super) loans: Vec<usize>,
}
impl Registry {
    pub(super) fn entry(source: &SourceOwners) -> Self {
        Self {
            completed: source
                .owners
                .iter()
                .filter_map(|owner| {
                    matches!(owner.origin, PrivateOrigin::Parameter(_))
                        .then_some(BoundaryOwner::Private(owner.origin))
                })
                .collect(),
            loans: Vec::new(),
        }
    }

    pub(super) fn completions(step: &FlowStep) -> Vec<BoundaryOwner> {
        match step {
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
        }
    }

    pub(super) fn advance(&mut self, step: &FlowStep) -> Result<(), BoundaryError> {
        for owner in Self::completions(step) {
            if self.completed.contains(&owner) {
                return Err(BoundaryError::owned("boundary-duplicate-completion"));
            }
            self.completed.push(owner);
        }
        match step {
            FlowStep::PrepareLoan { token, .. } => {
                if self.loans.contains(token) {
                    return Err(BoundaryError::owned("boundary-duplicate-loan"));
                }
                self.loans.push(*token);
            }
            FlowStep::ConfirmRelease { owner, .. } => {
                let index = self
                    .completed
                    .iter()
                    .position(|entry| *entry == BoundaryOwner::Foreign(*owner))
                    .ok_or_else(|| BoundaryError::owned("boundary-release-completion"))?;
                self.completed.remove(index);
            }
            _ => {}
        }
        Ok(())
    }
}

fn drops(
    registry: &Registry,
    source: &SourceOwners,
    foreign: &[CleanupEntry],
    protected: Option<PrivateOrigin>,
) -> Result<Vec<BoundaryDrop>, BoundaryError> {
    registry
        .completed
        .iter()
        .rev()
        .filter_map(|owner| match owner {
            BoundaryOwner::Private(origin) if Some(*origin) == protected => None,
            BoundaryOwner::Private(origin) => Some(
                source
                    .owners
                    .iter()
                    .find(|owner| owner.origin == *origin)
                    .cloned()
                    .map(BoundaryDrop::Private)
                    .ok_or_else(|| BoundaryError::owned("boundary-drop-private-origin")),
            ),
            BoundaryOwner::Foreign(owner) => foreign
                .iter()
                .find(|entry| entry.owner_id() == *owner)
                .cloned()
                .map(BoundaryDrop::Foreign)
                .map(Ok),
        })
        .collect()
}

pub(super) fn produce(
    rules: &[ExitRule<'_>],
    registry: &Registry,
    source: &SourceOwners,
    returned: Option<PrivateOrigin>,
) -> Result<Vec<BoundaryExit>, BoundaryError> {
    rules
        .iter()
        .map(|rule| {
            let protected = (rule.kind == BoundaryExitKind::Return).then_some(returned).flatten();
            let (cleanup, mut unresolved) = if rule.stop || !rule.cleanup_required {
                (Vec::new(), registry.completed.iter().rev().copied().collect::<Vec<_>>())
            } else {
                (drops(registry, source, rule.foreign, protected)?, Vec::new())
            };
            unresolved.extend_from_slice(&rule.unresolved);
            Ok(BoundaryExit {
                kind: rule.kind.clone(),
                end_loans: if rule.cleanup_required {
                    registry.loans.iter().rev().copied().collect()
                } else {
                    Vec::new()
                },
                cleanup,
                unresolved,
                protected_result: protected,
                cleanup_required: rule.cleanup_required,
                preparation_nonempty_only: rule.nonempty,
                release_failure_route: FailureRoute::ReleaseFailureOverridesUnresolved,
            })
        })
        .collect()
}

pub(super) fn check(
    exits: &[BoundaryExit],
    rules: &[ExitRule<'_>],
    registry: &Registry,
    source: &SourceOwners,
    returned: Option<PrivateOrigin>,
) -> Result<(), BoundaryError> {
    if exits.len() != rules.len() {
        return Err(BoundaryError::source("boundary-exit-inventory"));
    }
    for (exit, rule) in exits.iter().zip(rules) {
        let protected = if rule.kind == BoundaryExitKind::Return { returned } else { None };
        if exit.kind != rule.kind
            || exit.protected_result != protected
            || exit.cleanup_required != rule.cleanup_required
            || exit.preparation_nonempty_only != rule.nonempty
            || exit.release_failure_route != FailureRoute::ReleaseFailureOverridesUnresolved
        {
            return Err(BoundaryError::owned("boundary-exit-domain"));
        }
        let loan_ends = if rule.cleanup_required {
            registry.loans.iter().rev().copied().collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if exit.end_loans != loan_ends {
            return Err(BoundaryError::owned("boundary-loan-end-order"));
        }
        if rule.stop || !rule.cleanup_required {
            let mut unresolved = registry.completed.iter().rev().copied().collect::<Vec<_>>();
            unresolved.extend_from_slice(&rule.unresolved);
            if !exit.cleanup.is_empty() || exit.unresolved != unresolved {
                return Err(BoundaryError::owned("boundary-unresolved-no-retry"));
            }
            continue;
        }
        if exit.unresolved != rule.unresolved {
            return Err(BoundaryError::owned("boundary-unresolved-origin"));
        }
        let expected_owners = registry
            .completed
            .iter()
            .rev()
            .copied()
            .filter(|owner| match owner {
                BoundaryOwner::Private(origin) => Some(*origin) != protected,
                BoundaryOwner::Foreign(owner) => {
                    rule.foreign.iter().any(|entry| entry.owner_id() == *owner)
                }
            })
            .collect::<Vec<_>>();
        if exit.cleanup.iter().map(BoundaryDrop::owner).collect::<Vec<_>>() != expected_owners {
            return Err(BoundaryError::owned("boundary-reverse-completion-order"));
        }
        let foreign = exit
            .cleanup
            .iter()
            .filter_map(|drop| match drop {
                BoundaryDrop::Foreign(entry) => Some(entry),
                BoundaryDrop::Private(_) => None,
            })
            .collect::<Vec<_>>();
        if foreign != rule.foreign.iter().collect::<Vec<_>>() {
            return Err(BoundaryError::owned("boundary-foreign-cleanup-projection"));
        }
        for drop in &exit.cleanup {
            if let BoundaryDrop::Private(owner) = drop
                && source.owners.iter().find(|expected| expected.origin == owner.origin)
                    != Some(owner)
            {
                return Err(BoundaryError::owned("boundary-private-release-issuer"));
            }
        }
    }
    Ok(())
}

pub(super) fn release_call(steps: &[FlowStep], index: usize) -> bool {
    matches!((steps.get(index), steps.get(index + 1)),
        (Some(FlowStep::Call { call, .. }), Some(FlowStep::ConfirmRelease { call: release, .. })) if call == release)
}

pub(super) fn returned(step: &FlowStep, source: &SourceOwners) -> Option<PrivateOrigin> {
    match step {
        FlowStep::Return { expression, .. } => source.origins[*expression],
        _ => None,
    }
}
