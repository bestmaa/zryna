//! Exact complete declaration admission and independent call/status/output replay.

use crate::{IrError, raw, require};
use std::collections::{BTreeMap, BTreeSet};
use zryna_semantics::native_c_v0::body::{
    FlowStep, MAX_LIVE_FOREIGN_OBLIGATIONS, TrapRequirement, TypedFunction,
    VerifiedPrivateBoundaries,
};
use zryna_syntax::native_c_v0::{self, raw as declaration};

pub(super) fn preflight_effect(step: &FlowStep) -> Result<(), IrError> {
    use zryna_ir::data_ownership_v1::MAX_PLACES_PER_FUNCTION;
    let (cleanup, extra) = match step {
        FlowStep::OutputSlot { .. } | FlowStep::ReadOutput { .. } => (0, 0),
        FlowStep::PrepareLoan { traps, cleanup, .. } => {
            super::count(0, traps.len(), 3, "ir-preparation-trap-budget")?;
            (cleanup.len(), 0)
        }
        FlowStep::Reserve { cleanup, .. }
        | FlowStep::Copy { cleanup, .. }
        | FlowStep::Return { cleanup, .. } => (cleanup.len(), 0),
        FlowStep::Call {
            carriers,
            outputs,
            created_owners,
            recoverable,
            boundary_checks,
            unknown_status_unresolved_owners,
            cleanup,
            ..
        } => {
            super::count(0, carriers.len(), 16, "ir-call-carrier-budget")?;
            super::count(0, outputs.len(), 16, "ir-call-output-budget")?;
            super::count(0, created_owners.len(), 8, "ir-call-acquisition-budget")?;
            super::count(0, recoverable.len(), 16, "ir-call-status-budget")?;
            super::count(0, boundary_checks.len(), 25, "ir-boundary-check-budget")?;
            (cleanup.len(), unknown_status_unresolved_owners.len())
        }
        FlowStep::StatusGuard { recoverable, cleanup, .. } => {
            super::count(0, recoverable.len(), 16, "ir-guard-status-budget")?;
            (cleanup.len(), 0)
        }
        FlowStep::Take { slots, cleanup, .. } => {
            super::count(0, slots.len(), 16, "ir-take-slot-budget")?;
            (cleanup.len(), 0)
        }
        FlowStep::ConfirmRelease { unresolved_on_fault, .. } => (0, unresolved_on_fault.len()),
    };
    super::count(0, cleanup, MAX_PLACES_PER_FUNCTION, "ir-flow-cleanup-budget")?;
    super::count(0, extra, MAX_PLACES_PER_FUNCTION, "ir-flow-unresolved-budget")?;
    Ok(())
}

pub(super) fn declarations(
    program: &raw::Program,
    authority: &VerifiedPrivateBoundaries,
) -> Result<(), IrError> {
    let original = native_c_v0::decode(
        authority.body_authority().declaration_authority().declaration_bytes(),
        native_c_v0::TARGET,
    )
    .map_err(|e| IrError::new(e.code(), e.detail()))?;
    let candidate = &program.declarations;
    let mut document = serde_json::to_value(candidate)
        .map_err(|_| IrError::new("ZRYNA-C4101", "ir-declaration-shape"))?;
    document.sort_all_objects();
    let mut wire = serde_json::to_vec(&document)
        .map_err(|_| IrError::new("ZRYNA-C4100", "ir-declaration-wire"))?;
    wire.push(b'\n');
    let decoded = native_c_v0::decode(&wire, native_c_v0::TARGET);
    if let Err(e) = &decoded
        && matches!(e.code(), "ZRYNA-C4100" | "ZRYNA-C4101" | "ZRYNA-C4107")
    {
        return Err(IrError::new(e.code(), e.detail()));
    }
    require(
        program.source_map == authority.body_authority().source_map_identity()
            && candidate.sources == original.sources
            && candidate.libraries.len() == original.libraries.len()
            && candidate.operations.len() == original.operations.len(),
        "ZRYNA-C4102",
        "ir-declaration-inventory-identity",
    )?;
    for (claim, sealed) in candidate.libraries.iter().zip(&original.libraries) {
        require(
            claim.id == sealed.id
                && claim.version == sealed.version
                && claim.header_sha256 == sealed.header_sha256
                && claim.policy_sha256 == sealed.policy_sha256,
            "ZRYNA-C4102",
            "ir-captured-library-identity",
        )?;
    }
    for (claim, sealed) in candidate.operations.iter().zip(&original.operations) {
        require(
            claim.key == sealed.key
                && claim.library == sealed.library
                && claim.symbol == sealed.symbol
                && claim.logical_name == sealed.logical_name
                && claim.source_binding == sealed.source_binding,
            "ZRYNA-C4102",
            "ir-operation-identity",
        )?;
    }
    super::storage::identity(program, authority)?;
    decoded.map_err(|e| IrError::new(e.code(), e.detail()))?;
    contract(candidate, &original)
}

fn contract(
    candidate: &declaration::DeclarationSet,
    original: &declaration::DeclarationSet,
) -> Result<(), IrError> {
    require(candidate.target == original.target, "ZRYNA-C4103", "ir-native-target")?;
    require(
        candidate.abi == original.abi
            && candidate.convention == original.convention
            && candidate.carriers == original.carriers
            && candidate.runtime_contract == original.runtime_contract,
        "ZRYNA-C4104",
        "ir-abi-tuple",
    )?;
    for (claim, sealed) in candidate.operations.iter().zip(&original.operations) {
        require(
            claim.direction == sealed.direction
                && claim.parameters.len() == sealed.parameters.len()
                && claim
                    .parameters
                    .iter()
                    .zip(&sealed.parameters)
                    .all(|(a, b)| a.abi == b.abi && a.name == b.name)
                && claim.result == sealed.result
                && claim.effects == sealed.effects
                && claim.mode == sealed.mode
                && claim.execution == sealed.execution,
            "ZRYNA-C4104",
            "ir-operation-signature",
        )?;
    }
    for (claim, sealed) in candidate.operations.iter().zip(&original.operations) {
        require(
            claim.resources == sealed.resources
                && claim.statuses == sealed.statuses
                && claim
                    .parameters
                    .iter()
                    .zip(&sealed.parameters)
                    .all(|(a, b)| a.resource == b.resource),
            "ZRYNA-C4105",
            "ir-operation-resources",
        )?;
    }
    require(
        candidate.libraries == original.libraries && candidate.ownership == original.ownership,
        "ZRYNA-C4105",
        "ir-library-resource-policy",
    )?;
    require(candidate.sites == original.sites, "ZRYNA-C4106", "ir-exact-source-sites")?;
    // Equality covers every schema field after category-specific admission, including unused records.
    require(candidate == original, "ZRYNA-C4102", "ir-complete-declaration-contract")
}

pub(super) fn function(
    claim: &raw::Function,
    original: &TypedFunction,
    declarations: &declaration::DeclarationSet,
) -> Result<(), IrError> {
    let mut reservation = None;
    let mut calls = BTreeMap::new();
    let mut outputs = BTreeMap::new();
    let mut successful = BTreeSet::new();
    let mut live = BTreeSet::new();
    for (effect, source) in claim.effects.iter().zip(original.steps()) {
        source_flow(&effect.operation, source)?;
        match &effect.operation {
            FlowStep::OutputSlot { token, ty, .. } => {
                require(
                    outputs.insert(*token, (*ty, None)).is_none(),
                    "ZRYNA-C4105",
                    "ir-distinct-output-slot",
                )?;
            }
            FlowStep::Reserve { call, maximum_new_owners, live_limit, trap, .. } => {
                require(
                    reservation.replace((*call, *maximum_new_owners)).is_none()
                        && *live_limit == MAX_LIVE_FOREIGN_OBLIGATIONS
                        && *trap == TrapRequirement::ForeignResourceLimit,
                    "ZRYNA-C4105",
                    "ir-pre-effect-reservation",
                )?;
            }
            step @ FlowStep::Call { .. } => {
                replay_call(
                    step,
                    declarations,
                    &mut reservation,
                    &mut calls,
                    &mut outputs,
                    &mut live,
                    &mut successful,
                )?;
            }
            FlowStep::StatusGuard { call, recoverable, .. } => {
                let operation = calls
                    .get(call)
                    .and_then(|id| declarations.operations.get(*id))
                    .ok_or_else(|| IrError::new("ZRYNA-C4105", "ir-status-call-dominance"))?;
                require(
                    operation.mode == declaration::Mode::Status
                        && recoverable.iter().copied().eq(operation
                            .statuses
                            .iter()
                            .filter(|s| s.kind == declaration::StatusKind::Recoverable)
                            .map(|s| s.code)),
                    "ZRYNA-C4105",
                    "ir-guard-status-domain",
                )?;
                successful.insert(*call);
            }
            FlowStep::ReadOutput { slot, call, .. } => {
                require(
                    successful.contains(call)
                        && outputs.get(slot).is_some_and(|s| s.1 == Some(*call)),
                    "ZRYNA-C4105",
                    "ir-initialized-output-read",
                )?;
            }
            FlowStep::Take { owner, slots, call, .. } => {
                require(
                    successful.contains(call)
                        && live.contains(owner)
                        && slots
                            .iter()
                            .all(|slot| outputs.get(slot).is_some_and(|s| s.1 == Some(*call))),
                    "ZRYNA-C4105",
                    "ir-successful-resource-take",
                )?;
            }
            FlowStep::Copy { owner, .. } => {
                require(live.contains(owner), "ZRYNA-C4105", "ir-live-copy-source")?;
            }
            FlowStep::ConfirmRelease { call, operation, owner, .. } => {
                require(
                    calls.get(call) == Some(operation) && live.remove(owner),
                    "ZRYNA-C4105",
                    "ir-confirmed-matching-release",
                )?;
                let origin = original
                    .owner_origins()
                    .get(*owner)
                    .ok_or_else(|| IrError::new("ZRYNA-C4105", "ir-release-owner-origin"))?;
                require(
                    declarations.operations[*operation].key == origin.release_key()
                        && declarations.operations[*operation].library == origin.library(),
                    "ZRYNA-C4105",
                    "ir-same-library-release",
                )?;
            }
            _ => {}
        }
    }
    require(reservation.is_none(), "ZRYNA-C4105", "ir-unconsumed-reservation")
}

type OutputState = BTreeMap<usize, (zryna_semantics::native_c_v0::body::ValueType, Option<usize>)>;
fn replay_call(
    step: &FlowStep,
    declarations: &declaration::DeclarationSet,
    reservation: &mut Option<(usize, usize)>,
    calls: &mut BTreeMap<usize, usize>,
    outputs: &mut OutputState,
    live: &mut BTreeSet<usize>,
    successful: &mut BTreeSet<usize>,
) -> Result<(), IrError> {
    let FlowStep::Call {
        call,
        operation,
        carriers,
        outputs: slots,
        created_owners,
        recoverable,
        ..
    } = step
    else {
        return Err(IrError::new("ZRYNA-C4105", "ir-import-ordinal"));
    };
    let imported = declarations
        .operations
        .get(*operation)
        .ok_or_else(|| IrError::new("ZRYNA-C4105", "ir-import-ordinal"))?;
    require(
        imported.direction == declaration::Direction::Import
            && reservation.take() == Some((*call, created_owners.len()))
            && calls.insert(*call, *operation).is_none(),
        "ZRYNA-C4105",
        "ir-exact-reserved-call",
    )?;
    require(
        carriers.iter().copied().eq(imported.parameters.iter().map(|p| p.abi)),
        "ZRYNA-C4104",
        "ir-call-carriers",
    )?;
    require(
        recoverable.iter().copied().eq(imported
            .statuses
            .iter()
            .filter(|s| s.kind == declaration::StatusKind::Recoverable)
            .map(|s| s.code)),
        "ZRYNA-C4105",
        "ir-exact-status-domain",
    )?;
    let distinct: BTreeSet<_> = slots.iter().copied().collect();
    require(distinct.len() == slots.len(), "ZRYNA-C4105", "ir-output-alias")?;
    for slot in slots {
        let entry = outputs
            .get_mut(slot)
            .ok_or_else(|| IrError::new("ZRYNA-C4105", "ir-output-created-before-call"))?;
        entry.1 = Some(*call);
    }
    for owner in created_owners {
        require(live.insert(*owner), "ZRYNA-C4105", "ir-fresh-conditional-owner")?;
    }
    if imported.mode != declaration::Mode::Status {
        successful.insert(*call);
    }
    Ok(())
}

fn source_flow(claim: &FlowStep, source: &FlowStep) -> Result<(), IrError> {
    if let (
        FlowStep::Call { safety, carriers, .. },
        FlowStep::Call { safety: actual_safety, carriers: actual_carriers, .. },
    ) = (claim, source)
    {
        require(carriers == actual_carriers, "ZRYNA-C4104", "ir-source-call-carriers")?;
        require(safety == actual_safety, "ZRYNA-C4106", "ir-explicit-call-safety")?;
    }
    require(claim == source, "ZRYNA-C4105", "ir-source-call-flow")?;
    Ok(())
}
