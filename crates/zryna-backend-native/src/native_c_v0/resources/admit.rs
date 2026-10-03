//! Closed executable subset and finite emission inventory; the original seal remains complete.

use super::super::invariant_error;
use std::collections::BTreeSet;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::{
    VerifiedFunction, VerifiedMirProgram,
    contract::{AbiType, BoundaryDrop, FlowStep, SourceType, ValueType},
};

pub(super) fn entries(
    program: &VerifiedMirProgram,
    names: &[&str],
) -> Result<Vec<usize>, Diagnostic> {
    select(program, names, false)
}

pub(super) fn storage_entries(
    program: &VerifiedMirProgram,
    names: &[&str],
) -> Result<Vec<usize>, Diagnostic> {
    select(program, names, true)
}
fn select(
    program: &VerifiedMirProgram,
    names: &[&str],
    storage: bool,
) -> Result<Vec<usize>, Diagnostic> {
    if names.is_empty() || names.len() > program.functions().len() {
        return Err(invariant_error());
    }
    let requested = names.iter().copied().collect::<BTreeSet<_>>();
    if requested.len() != names.len() {
        return Err(invariant_error());
    }
    if program.operations().any(|operation| {
        operation.declaration().symbol.to_ascii_lowercase().starts_with("zryna_c_v0_r_release_")
            || super::ledger::SYMBOLS
                .iter()
                .any(|name| operation.declaration().symbol.eq_ignore_ascii_case(name))
    }) {
        return Err(invariant_error());
    }
    if program.operations().any(|operation| {
        program
            .source()
            .runtime_abi()
            .native_linux_x86_64_functions()
            .any(|runtime| runtime.symbol().eq_ignore_ascii_case(&operation.declaration().symbol))
            || operation.declaration().symbol.eq_ignore_ascii_case(super::storage::UTF8)
    }) {
        return Err(invariant_error());
    }
    let mut selected = Vec::new();
    let mut total_units = 0_usize;
    for (ordinal, function) in program.functions().enumerate() {
        if requested.contains(function.entry().symbol.as_str()) {
            total_units = total_units
                .checked_add(check(function, program, storage)?)
                .ok_or_else(invariant_error)?;
            if total_units > 1_000_000 {
                return Err(invariant_error());
            }
            selected.push(ordinal);
        }
    }
    if selected.len() != names.len() {
        return Err(invariant_error());
    }
    Ok(selected)
}
fn check(
    function: VerifiedFunction<'_>,
    program: &VerifiedMirProgram,
    storage: bool,
) -> Result<usize, Diagnostic> {
    if function.parameters().len() > 16
        || !(matches!(function.result().1, ValueType::I32 | ValueType::Bool)
            || storage && matches!(function.result().1, ValueType::VecI32 | ValueType::String))
        || (!storage && !function.private_owners().is_empty())
        || function.bindings().iter().any(|binding| !is_parameter(binding.ty, storage))
        || function.values().iter().any(|value| !is_value(value.ty, storage))
    {
        return Err(invariant_error());
    }
    let operations = program.operations().collect::<Vec<_>>();
    let mut units = function.values().len();
    for effect in function.effects() {
        units = units.checked_add(effect.instructions().len()).ok_or_else(invariant_error)?;
        for exit in effect.exits() {
            if !storage
                && (!exit.end_loans.is_empty()
                    || exit.protected_result.is_some()
                    || exit.cleanup.iter().any(|drop| matches!(drop, BoundaryDrop::Private(_))))
            {
                return Err(invariant_error());
            }
            units = units
                .checked_add(exit.cleanup.len().checked_mul(32).ok_or_else(invariant_error)?)
                .ok_or_else(invariant_error)?;
        }
        if units > 1_000_000 {
            return Err(invariant_error());
        }
        match effect.operation() {
            FlowStep::PrepareLoan { .. } | FlowStep::Copy { .. } if !storage => {
                return Err(invariant_error());
            }
            FlowStep::Call { operation, created_owners, .. } => {
                if created_owners.len() > 1 {
                    return Err(invariant_error());
                }
                let declaration =
                    operations.get(*operation).ok_or_else(invariant_error)?.declaration();
                if !created_owners.is_empty()
                    && declaration
                        .parameters
                        .iter()
                        .any(|parameter| parameter.abi == AbiType::HandleIn)
                {
                    return Err(invariant_error());
                }
                if declaration
                    .parameters
                    .iter()
                    .any(|parameter| !is_carrier(parameter.abi, storage))
                {
                    return Err(invariant_error());
                }
            }
            _ => {}
        }
    }
    Ok(units)
}

pub(super) fn imports(program: &VerifiedMirProgram, selected: &[usize]) -> BTreeSet<usize> {
    let mut imported = BTreeSet::new();
    for (index, function) in program.functions().enumerate() {
        if !selected.contains(&index) {
            continue;
        }
        for effect in function.effects() {
            if let FlowStep::Call { operation, .. } = effect.operation() {
                imported.insert(*operation);
            }
            for exit in effect.exits() {
                for drop in &exit.cleanup {
                    if let BoundaryDrop::Foreign(owner) = drop
                        && let Some(release) = release_for(function, program, owner.owner_id())
                    {
                        imported.insert(release);
                    }
                }
            }
        }
    }
    imported
}

pub(super) fn release_for(
    function: VerifiedFunction<'_>,
    program: &VerifiedMirProgram,
    owner: usize,
) -> Option<usize> {
    let operations = program.operations().collect::<Vec<_>>();
    for effect in function.effects() {
        if let FlowStep::Call { operation, created_owners, .. } = effect.operation()
            && let Some(position) = created_owners.iter().position(|id| *id == owner)
        {
            let declaration = operations.get(*operation)?.declaration();
            let resource =
                declaration.resources.iter().filter(|resource| resource.fresh).nth(position)?;
            return operations
                .iter()
                .position(|candidate| candidate.declaration().key == resource.release);
        }
    }
    None
}

fn is_parameter(ty: SourceType, storage: bool) -> bool {
    matches!(ty, SourceType::I32 | SourceType::Bool)
        || storage && matches!(ty, SourceType::String | SourceType::VecI32)
}
fn is_value(ty: ValueType, storage: bool) -> bool {
    matches!(
        ty,
        ValueType::I32
            | ValueType::Bool
            | ValueType::Key
            | ValueType::Unit
            | ValueType::Terminal
            | ValueType::I32Out
            | ValueType::HandleOut
            | ValueType::Handle
    ) || storage
        && matches!(
            ty,
            ValueType::String
                | ValueType::VecI32
                | ValueType::Bytes
                | ValueType::BytesOut
                | ValueType::CountOut
                | ValueType::OwnedBytes
        )
}
fn is_carrier(ty: AbiType, storage: bool) -> bool {
    matches!(
        ty,
        AbiType::CI32
            | AbiType::CInt
            | AbiType::Bool32
            | AbiType::I32Out
            | AbiType::HandleIn
            | AbiType::HandleOut
    ) || storage
        && matches!(
            ty,
            AbiType::Count
                | AbiType::BytesIn
                | AbiType::BytesOwnedOut
                | AbiType::CountOut
                | AbiType::BytesRelease
        )
}
