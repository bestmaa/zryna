//! Checked returned-byte expansion while the original foreign owner remains live.

use super::super::{FlowStep, ValueType};
use super::{BoundaryError, LayoutAuthority, PrivateCopy, PrivateOrigin, StorageStage};
use zryna_layout::StorageTarget;
use zryna_ownership_runtime_abi::LogicalOperation;

pub(super) fn derive(
    step: &FlowStep,
    layouts: &LayoutAuthority,
) -> Result<PrivateCopy, BoundaryError> {
    let FlowStep::Copy { expression, owner, .. } = step else {
        return Err(BoundaryError::source("boundary-copy-source-step"));
    };
    let (stride, alignment) = layouts.element(StorageTarget::LinuxX8664V1)?;
    Ok(PrivateCopy {
        expression: *expression,
        foreign_owner: *owner,
        result: PrivateOrigin::Copy(*expression),
        vector_type: layouts.type_id(ValueType::VecI32)?,
        element_type: layouts.type_id(ValueType::I32)?,
        stride,
        alignment,
        allocation: layouts.operation(LogicalOperation::VecAllocate)?,
        faults: layouts.faults(LogicalOperation::VecAllocate)?,
        stages: vec![
            StorageStage::ValidateForeign,
            StorageStage::CheckedCapacity,
            StorageStage::AllocateNonempty,
            StorageStage::Initialize,
            StorageStage::Commit,
        ],
        zero_extend_bytes: true,
        empty_without_allocation: true,
    })
}

pub(super) fn check(
    copy: &PrivateCopy,
    step: &FlowStep,
    layouts: &LayoutAuthority,
) -> Result<(), BoundaryError> {
    let FlowStep::Copy { expression, owner, .. } = step else {
        return Err(BoundaryError::source("boundary-unexpected-copy"));
    };
    let element = layouts.type_id(ValueType::I32)?;
    let vector = layouts.type_id(ValueType::VecI32)?;
    let (stride, alignment) = layouts.element(StorageTarget::LinuxX8664V1)?;
    if copy.expression != *expression
        || copy.foreign_owner != *owner
        || copy.result != PrivateOrigin::Copy(*expression)
        || copy.vector_type != vector
        || copy.element_type != element
        || copy.stride != stride
        || stride != 4
        || copy.alignment != alignment
    {
        return Err(BoundaryError::typed("boundary-copy-element-layout"));
    }
    if copy.allocation != layouts.operation(LogicalOperation::VecAllocate)?
        || copy.faults != layouts.faults(LogicalOperation::VecAllocate)?
    {
        return Err(BoundaryError::owned("boundary-copy-private-issuer"));
    }
    if !copy.zero_extend_bytes
        || !copy.empty_without_allocation
        || copy.stages
            != [
                StorageStage::ValidateForeign,
                StorageStage::CheckedCapacity,
                StorageStage::AllocateNonempty,
                StorageStage::Initialize,
                StorageStage::Commit,
            ]
    {
        return Err(BoundaryError::owned("boundary-copy-initialized-prefix"));
    }
    Ok(())
}
