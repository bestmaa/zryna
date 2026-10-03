//! Retained dual-layout/runtime issuer and explicit physical preparation obligations.

use crate::{IrError, raw, require};
use zryna_layout::{StorageTarget, TypeCategory, TypeId};
use zryna_ownership_runtime_abi::{
    LogicalOperation, OperationIdentity, RuntimeStatus, VerifiedStatusDisposition,
};
use zryna_semantics::native_c_v0::body::{
    FlowStep, PrivateFault, PrivateOrigin, PrivatePreparation, StorageStage as S, ValueType,
    VerifiedPrivateBoundaries,
};

pub(super) fn preflight_preparation(
    preparation: Option<&PrivatePreparation>,
) -> Result<(), IrError> {
    let (faults, stages) = match preparation {
        Some(PrivatePreparation::Loan(loan)) => (loan.faults.len(), loan.stages.len()),
        Some(PrivatePreparation::Copy(copy)) => (copy.faults.len(), copy.stages.len()),
        None => (0, 0),
    };
    super::count(0, faults, 2, "ir-private-fault-budget")?;
    super::count(0, stages, 5, "ir-storage-stage-budget")?;
    Ok(())
}

pub(super) fn identity(
    program: &raw::Program,
    authority: &VerifiedPrivateBoundaries,
) -> Result<(), IrError> {
    let native = authority.native_layouts();
    let linear = authority.linear_layouts();
    let runtime = authority.runtime_abi();
    require(
        native.target() == StorageTarget::LinuxX8664V1
            && linear.target() == StorageTarget::Linear32V1
            && native.source_map_identity() == program.source_map
            && linear.source_map_identity() == program.source_map
            && native.universe_identity() == linear.universe_identity()
            && runtime.type_universe_identity() == native.universe_identity()
            && runtime.linear32_fingerprint() == *linear.fingerprint()
            && runtime.linux_x86_64_fingerprint() == *native.fingerprint(),
        "ZRYNA-C4102",
        "ir-retained-layout-runtime-issuer",
    )?;
    require(
        program.storage.universe == native.universe_identity().as_bytes()
            && program.storage.linear == *linear.fingerprint()
            && program.storage.native == *native.fingerprint()
            && program.storage.runtime == runtime.identifier(),
        "ZRYNA-C4102",
        "ir-storage-descriptors",
    )?;
    Ok(())
}

pub(super) fn check(
    program: &raw::Program,
    authority: &VerifiedPrivateBoundaries,
) -> Result<(), IrError> {
    identity(program, authority)?;
    let native = authority.native_layouts();
    let runtime = authority.runtime_abi();
    let i32_type = type_id(authority, ValueType::I32)?;
    let vector = type_id(authority, ValueType::VecI32)?;
    require(
        native.type_by_id(vector).and_then(zryna_layout::VerifiedType::referenced_type)
            == Some(i32_type),
        "ZRYNA-C4104",
        "ir-private-vector-element",
    )?;
    for target in [StorageTarget::Linear32V1, StorageTarget::LinuxX8664V1] {
        let element = runtime
            .element_layouts()
            .find(|e| e.target() == target && e.element() == i32_type)
            .ok_or_else(|| IrError::new("ZRYNA-C4102", "ir-element-layout-issuer"))?;
        require(
            element.stride() == 4 && element.alignment() == 4,
            "ZRYNA-C4104",
            "ir-i32-element-layout",
        )?;
    }
    for (function, sealed) in program.functions.iter().zip(authority.functions()) {
        require(
            function.parameter_layouts == sealed.parameter_types
                && function.result_layout == sealed.result_type
                && function.private_owners == sealed.private_owners,
            "ZRYNA-C4105",
            "ir-private-storage-inventory",
        )
        .map_err(|e| e.at(function.span))?;
        for (parameter, claimed) in function.parameters.iter().zip(&function.parameter_layouts) {
            require(
                *claimed == type_id(authority, parameter.ty.into())?,
                "ZRYNA-C4104",
                "ir-private-input-layout",
            )?;
        }
        require(
            function.result_layout == type_id(authority, function.result)?,
            "ZRYNA-C4104",
            "ir-private-result-layout",
        )?;
        for owner in &function.private_owners {
            let operation = match owner
                .ty
                .and_then(|id| native.type_by_id(id))
                .map(zryna_layout::VerifiedType::category)
            {
                Some(TypeCategory::String) => LogicalOperation::StringRelease,
                Some(TypeCategory::Vec) => LogicalOperation::VecReleaseStorage,
                None if owner.ty.is_none() && matches!(owner.origin, PrivateOrigin::Packed(_)) => {
                    LogicalOperation::Release
                }
                _ => return Err(IrError::new("ZRYNA-C4105", "ir-private-owner-layout")),
            };
            require(
                owner.release == operation_id(authority, operation)? && owner.nonempty_storage_only,
                "ZRYNA-C4105",
                "ir-private-release-issuer",
            )?;
        }
        for (effect, original) in function.effects.iter().zip(&sealed.steps) {
            require(
                effect.preparation == original.preparation,
                "ZRYNA-C4105",
                "ir-authenticated-preparation",
            )?;
            match (&effect.operation, &effect.preparation) {
                (step @ FlowStep::PrepareLoan { .. }, Some(PrivatePreparation::Loan(loan))) => {
                    check_loan(function, step, loan, authority)?;
                }
                (step @ FlowStep::Copy { .. }, Some(PrivatePreparation::Copy(copy))) => {
                    check_copy(step, copy, vector, i32_type, authority)?;
                }
                (FlowStep::PrepareLoan { .. } | FlowStep::Copy { .. }, _) | (_, Some(_)) => {
                    return Err(IrError::new("ZRYNA-C4105", "ir-preparation-site"));
                }
                (_, None) => {}
            }
        }
    }
    Ok(())
}

fn type_id(authority: &VerifiedPrivateBoundaries, ty: ValueType) -> Result<TypeId, IrError> {
    let category = match ty {
        ValueType::I32 => TypeCategory::I32,
        ValueType::Bool => TypeCategory::Bool,
        ValueType::String => TypeCategory::String,
        ValueType::VecI32 => TypeCategory::Vec,
        _ => return Err(IrError::new("ZRYNA-C4104", "ir-private-boundary-category")),
    };
    let mut matches =
        authority.native_layouts().types().filter(|record| record.category() == category);
    let found =
        matches.next().ok_or_else(|| IrError::new("ZRYNA-C4104", "ir-private-layout-category"))?;
    require(matches.next().is_none(), "ZRYNA-C4104", "ir-ambiguous-private-layout")?;
    Ok(found.id())
}

fn operation_id(
    authority: &VerifiedPrivateBoundaries,
    logical: LogicalOperation,
) -> Result<OperationIdentity, IrError> {
    authority
        .runtime_abi()
        .operations()
        .find(|operation| operation.operation() == logical)
        .map(zryna_ownership_runtime_abi::VerifiedOperation::id)
        .ok_or_else(|| IrError::new("ZRYNA-C4105", "ir-private-operation-issuer"))
}

fn faults(
    claims: &[PrivateFault],
    operation: OperationIdentity,
    authority: &VerifiedPrivateBoundaries,
) -> Result<(), IrError> {
    require(claims.len() == 2, "ZRYNA-C4105", "ir-private-fault-inventory")?;
    for (claim, status) in claims.iter().zip([RuntimeStatus::Allocation, RuntimeStatus::Capacity]) {
        let declaration = authority
            .runtime_abi()
            .status_declarations()
            .find(|d| d.status() == status)
            .ok_or_else(|| IrError::new("ZRYNA-C4105", "ir-private-status-issuer"))?;
        require(
            claim.runtime == authority.runtime_abi().identity()
                && claim.operation == operation
                && claim.declaration == declaration
                && declaration.disposition() == VerifiedStatusDisposition::ControlledTrap
                && declaration.trap_identity().is_some(),
            "ZRYNA-C4105",
            "ir-private-trap-domain",
        )?;
    }
    Ok(())
}

fn check_loan(
    function: &raw::Function,
    step: &FlowStep,
    loan: &zryna_semantics::native_c_v0::body::PrivateLoan,
    authority: &VerifiedPrivateBoundaries,
) -> Result<(), IrError> {
    let FlowStep::PrepareLoan { expression, token, source_expression, utf8, maximum_bytes, .. } =
        step
    else {
        return Err(IrError::new("ZRYNA-C4105", "ir-preparation-site"));
    };
    require(
        loan.expression == *expression
            && loan.token == *token
            && loan.maximum_bytes == *maximum_bytes
            && *maximum_bytes == 4096
            && function
                .values
                .get(*source_expression)
                .is_some_and(|v| v.origin == Some(loan.source))
            && loan.source_type
                == type_id(authority, if *utf8 { ValueType::String } else { ValueType::VecI32 })?
            && loan.backing_stride == 1
            && loan.backing_alignment == 1
            && loan.native_bits == 64
            && loan.empty_without_allocation,
        "ZRYNA-C4105",
        "ir-retained-byte-loan",
    )?;
    if *utf8 {
        require(
            loan.scratch.is_none()
                && loan.allocation.is_none()
                && loan.faults.is_empty()
                && loan.source_stride == 1
                && loan.stages == [S::Utf8, S::Length],
            "ZRYNA-C4105",
            "ir-string-loan-without-allocation",
        )?;
    } else {
        let allocation = operation_id(authority, LogicalOperation::Allocate)?;
        require(
            loan.source_stride == 4
                && loan.scratch == Some(PrivateOrigin::Packed(*expression))
                && loan.allocation == Some(allocation)
                && loan.stages
                    == [S::Length, S::ByteRange, S::AllocateNonempty, S::Initialize, S::Commit],
            "ZRYNA-C4105",
            "ir-distinct-byte-packing",
        )?;
        faults(&loan.faults, allocation, authority)?;
    }
    Ok(())
}

fn check_copy(
    step: &FlowStep,
    copy: &zryna_semantics::native_c_v0::body::PrivateCopy,
    vector: TypeId,
    i32_type: TypeId,
    authority: &VerifiedPrivateBoundaries,
) -> Result<(), IrError> {
    let FlowStep::Copy { expression, owner, .. } = step else {
        return Err(IrError::new("ZRYNA-C4105", "ir-preparation-site"));
    };
    let allocation = operation_id(authority, LogicalOperation::VecAllocate)?;
    require(
        copy.expression == *expression
            && copy.foreign_owner == *owner
            && copy.result == PrivateOrigin::Copy(*expression)
            && copy.vector_type == vector
            && copy.element_type == i32_type
            && copy.stride == 4
            && copy.alignment == 4
            && copy.allocation == allocation
            && copy.zero_extend_bytes
            && copy.empty_without_allocation
            && copy.stages
                == [
                    S::ValidateForeign,
                    S::CheckedCapacity,
                    S::AllocateNonempty,
                    S::Initialize,
                    S::Commit,
                ],
        "ZRYNA-C4105",
        "ir-distinct-private-byte-copy",
    )?;
    faults(&copy.faults, allocation, authority)?;
    Ok(())
}
