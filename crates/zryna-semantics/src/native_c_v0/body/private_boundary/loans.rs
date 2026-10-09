//! Separate packed byte storage and retained String storage; never pointer reinterpretation.

use super::super::{FlowStep, ValueType};
use super::owners::SourceOwners;
use super::{BoundaryError, LayoutAuthority, PrivateLoan, PrivateOrigin, StorageStage};
use zryna_layout::StorageTarget;
use zryna_ownership_runtime_abi::LogicalOperation;

pub(super) fn derive(
    step: &FlowStep,
    source: &SourceOwners,
    layouts: &LayoutAuthority,
) -> Result<PrivateLoan, BoundaryError> {
    let FlowStep::PrepareLoan { expression, token, source_expression, utf8, maximum_bytes, .. } =
        step
    else {
        return Err(BoundaryError::source("boundary-loan-source-step"));
    };
    let origin = source
        .origins
        .get(*source_expression)
        .copied()
        .flatten()
        .ok_or_else(|| BoundaryError::owned("boundary-loan-source-origin"))?;
    let source_type = layouts.type_id(if *utf8 { ValueType::String } else { ValueType::VecI32 })?;
    let (scratch, allocation, faults, stages, source_stride) = if *utf8 {
        (None, None, Vec::new(), vec![StorageStage::Utf8, StorageStage::Length], 1)
    } else {
        (
            Some(PrivateOrigin::Packed(*expression)),
            Some(layouts.operation(LogicalOperation::Allocate)?),
            layouts.faults(LogicalOperation::Allocate)?,
            vec![
                StorageStage::Length,
                StorageStage::ByteRange,
                StorageStage::AllocateNonempty,
                StorageStage::Initialize,
                StorageStage::Commit,
            ],
            layouts.element(StorageTarget::LinuxX8664V1)?.0,
        )
    };
    Ok(PrivateLoan {
        expression: *expression,
        token: *token,
        source: origin,
        source_type,
        scratch,
        allocation,
        faults,
        stages,
        maximum_bytes: *maximum_bytes,
        source_stride,
        backing_stride: 1,
        backing_alignment: 1,
        native_bits: layouts.native_bits()?,
        empty_without_allocation: true,
    })
}

pub(super) fn check(
    loan: &PrivateLoan,
    step: &FlowStep,
    source: &SourceOwners,
    layouts: &LayoutAuthority,
) -> Result<(), BoundaryError> {
    let FlowStep::PrepareLoan { expression, token, source_expression, utf8, maximum_bytes, .. } =
        step
    else {
        return Err(BoundaryError::source("boundary-unexpected-loan"));
    };
    if loan.expression != *expression
        || loan.token != *token
        || loan.maximum_bytes != *maximum_bytes
        || loan.maximum_bytes != 4096
        || loan.native_bits != layouts.native_bits()?
        || loan.backing_stride != 1
        || loan.backing_alignment != 1
        || !loan.empty_without_allocation
        || source.origins.get(*source_expression) != Some(&Some(loan.source))
    {
        return Err(BoundaryError::owned("boundary-loan-retention"));
    }
    let ty = layouts.type_id(if *utf8 { ValueType::String } else { ValueType::VecI32 })?;
    if loan.source_type != ty
        || !source.owners.iter().any(|owner| owner.origin == loan.source && owner.ty == Some(ty))
    {
        return Err(BoundaryError::typed("boundary-loan-layout"));
    }
    if *utf8 {
        if loan.scratch.is_some()
            || loan.allocation.is_some()
            || !loan.faults.is_empty()
            || loan.source_stride != 1
            || loan.stages != [StorageStage::Utf8, StorageStage::Length]
        {
            return Err(BoundaryError::owned("boundary-string-storage-retention"));
        }
    } else if loan.source_stride != 4
        || loan.scratch != Some(PrivateOrigin::Packed(*expression))
        || loan.allocation != Some(layouts.operation(LogicalOperation::Allocate)?)
        || loan.faults != layouts.faults(LogicalOperation::Allocate)?
        || loan.stages
            != [
                StorageStage::Length,
                StorageStage::ByteRange,
                StorageStage::AllocateNonempty,
                StorageStage::Initialize,
                StorageStage::Commit,
            ]
    {
        return Err(BoundaryError::owned("boundary-byte-packing"));
    }
    Ok(())
}
