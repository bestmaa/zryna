//! Original source binding replay and checked preflight before candidate allocation.

use super::super::{FlowStep, TypedFunction, ValueType};
use super::{BoundaryError, FunctionBoundary, LayoutAuthority, PrivateOrigin, PrivateOwner};
use std::collections::BTreeMap;
use zryna_ir::data_ownership_v1 as limits;
use zryna_ownership_runtime_abi::LogicalOperation;
use zryna_syntax::{native_c_source_v0::raw as syntax, native_c_v0::raw::Primitive};

pub(super) struct SourceOwners {
    pub(super) origins: Vec<Option<PrivateOrigin>>,
    pub(super) owners: Vec<PrivateOwner>,
}

pub(super) fn private_owner(
    origin: PrivateOrigin,
    ty: ValueType,
    layouts: &LayoutAuthority,
) -> Result<PrivateOwner, BoundaryError> {
    let (ty, release) = match ty {
        ValueType::String => (Some(layouts.type_id(ty)?), LogicalOperation::StringRelease),
        ValueType::VecI32 => (Some(layouts.type_id(ty)?), LogicalOperation::VecReleaseStorage),
        ValueType::Bytes => (None, LogicalOperation::Release),
        _ => return Err(BoundaryError::typed("boundary-private-owner-type")),
    };
    let packed_size_from = match origin {
        PrivateOrigin::Packed(expression) => Some(expression),
        _ => None,
    };
    Ok(PrivateOwner {
        origin,
        ty,
        release: layouts.operation(release)?,
        nonempty_storage_only: true,
        packed_size_from,
        packed_alignment: packed_size_from.map(|_| 1),
    })
}

pub(super) fn derive(
    function: &TypedFunction,
    layouts: &LayoutAuthority,
) -> Result<SourceOwners, BoundaryError> {
    let mut bindings = BTreeMap::new();
    let mut owners = Vec::new();
    for (index, parameter) in function.parameters().iter().enumerate() {
        let ty = ValueType::from(parameter.ty);
        let origin = if matches!(ty, ValueType::String | ValueType::VecI32) {
            let origin = PrivateOrigin::Parameter(index);
            owners.push(private_owner(origin, ty, layouts)?);
            Some(origin)
        } else {
            None
        };
        bindings.insert(parameter.name.clone(), (origin, true));
    }
    let mut origins = Vec::with_capacity(function.expressions().len());
    for statement in function.statements() {
        let root = match &statement.kind {
            syntax::StatementKind::Const(_, root)
            | syntax::StatementKind::Return(root)
            | syntax::StatementKind::Guard(_, root)
            | syntax::StatementKind::Expression(root) => *root,
        };
        if root < origins.len() || root >= function.expressions().len() {
            return Err(BoundaryError::source("boundary-source-expression-order"));
        }
        while origins.len() <= root {
            let index = origins.len();
            let expression = &function.expressions()[index];
            let origin = match expression.source_kind() {
                syntax::ExpressionKind::Local(name) => {
                    let (origin, available) = bindings
                        .get(name)
                        .ok_or_else(|| BoundaryError::source("boundary-original-binding"))?;
                    if origin.is_some() && !available {
                        return Err(BoundaryError::owned("boundary-moved-private-binding"));
                    }
                    *origin
                }
                syntax::ExpressionKind::Intrinsic(Primitive::CopyBytes, _) => {
                    let origin = PrivateOrigin::Copy(index);
                    owners.push(private_owner(origin, ValueType::VecI32, layouts)?);
                    Some(origin)
                }
                syntax::ExpressionKind::Intrinsic(Primitive::BorrowBytes, _) => {
                    owners.push(private_owner(
                        PrivateOrigin::Packed(index),
                        ValueType::Bytes,
                        layouts,
                    )?);
                    None
                }
                _ => None,
            };
            origins.push(origin);
        }
        if let syntax::StatementKind::Const(binding, expression) = &statement.kind {
            let origin = origins[*expression];
            if origin.is_some()
                && let syntax::ExpressionKind::Local(name) =
                    function.expressions()[*expression].source_kind()
            {
                let previous = bindings
                    .get_mut(name)
                    .ok_or_else(|| BoundaryError::source("boundary-move-source"))?;
                previous.1 = false;
            }
            if bindings.insert(binding.name.clone(), (origin, true)).is_some() {
                return Err(BoundaryError::source("boundary-duplicate-binding"));
            }
        }
    }
    if origins.len() != function.expressions().len() {
        return Err(BoundaryError::source("boundary-incomplete-source-arena"));
    }
    Ok(SourceOwners { origins, owners })
}

pub(in crate::native_c_v0::body) fn checked_count(
    current: usize,
    additional: usize,
    maximum: usize,
) -> Result<usize, BoundaryError> {
    current
        .checked_add(additional)
        .filter(|value| *value <= maximum)
        .ok_or_else(|| BoundaryError::budget("boundary-compiler-budget"))
}

pub(super) fn preflight(functions: &[TypedFunction]) -> Result<(), BoundaryError> {
    let mut values = 0;
    for function in functions {
        let count = function.expressions().len();
        checked_count(0, count, limits::MAX_VALUES_PER_FUNCTION)?;
        values = checked_count(values, count, limits::MAX_VALUES_PER_PROGRAM)?;
        let preparations = function
            .expressions()
            .iter()
            .filter(|expression| {
                matches!(
                    expression.source_kind(),
                    syntax::ExpressionKind::Intrinsic(
                        Primitive::BorrowBytes | Primitive::BorrowUtf8 | Primitive::CopyBytes,
                        _
                    )
                )
            })
            .count();
        checked_count(0, preparations, limits::MAX_ACTIVE_BORROWS_PER_FUNCTION)?;
        let owners = checked_count(
            function.parameters().len(),
            preparations,
            limits::MAX_PLACES_PER_FUNCTION,
        )?;
        let owners =
            checked_count(owners, function.owner_origins().len(), limits::MAX_PLACES_PER_FUNCTION)?;
        let mut plans = 0;
        for step in function.steps() {
            let count = match step {
                FlowStep::PrepareLoan { utf8, .. } => {
                    if *utf8 {
                        1
                    } else {
                        6
                    }
                }
                FlowStep::Call { boundary_checks, .. } => {
                    checked_count(boundary_checks.len(), 2, limits::MAX_CLEANUP_PLANS_PER_FUNCTION)?
                }
                FlowStep::Copy { .. } => 4,
                FlowStep::OutputSlot { .. } | FlowStep::ReadOutput { .. } => 0,
                _ => 1,
            };
            plans = checked_count(plans, count, limits::MAX_CLEANUP_PLANS_PER_FUNCTION)?;
        }
        let drops = owners
            .checked_mul(plans)
            .ok_or_else(|| BoundaryError::budget("boundary-drop-multiplication"))?;
        checked_count(0, drops, limits::MAX_DROP_ACTIONS_PER_FUNCTION)?;
        checked_count(plans, owners, limits::MAX_OWNERSHIP_TRANSITIONS_PER_FUNCTION)?;
    }
    Ok(())
}

pub(super) fn check_candidate_counts(function: &FunctionBoundary) -> Result<(), BoundaryError> {
    checked_count(0, function.expression_origins.len(), limits::MAX_VALUES_PER_FUNCTION)?;
    checked_count(0, function.private_owners.len(), limits::MAX_PLACES_PER_FUNCTION)?;
    let mut plans = 0;
    let mut drops = 0;
    let mut transitions = function.steps.len();
    for step in &function.steps {
        plans = checked_count(plans, step.exits.len(), limits::MAX_CLEANUP_PLANS_PER_FUNCTION)?;
        transitions = checked_count(
            transitions,
            step.completed.len(),
            limits::MAX_OWNERSHIP_TRANSITIONS_PER_FUNCTION,
        )?;
        for exit in &step.exits {
            drops =
                checked_count(drops, exit.cleanup.len(), limits::MAX_DROP_ACTIONS_PER_FUNCTION)?;
            checked_count(0, exit.end_loans.len(), limits::MAX_ACTIVE_BORROWS_PER_FUNCTION)?;
        }
    }
    Ok(())
}
