//! Independent slot allocation and machine instruction ordering from retained source effects.

use super::super::{MirError, abi::OutputSlot, raw, require};
use std::collections::BTreeMap;
use zryna_native_c_ir::{
    VerifiedFunction,
    contract::{AbiType, BoundaryCheck, FlowStep, Mode, Primitive, PrivatePreparation, ValueType},
    raw::ValueKind,
};

pub(super) fn check(
    function: &raw::Function,
    source: VerifiedFunction<'_>,
    operations: &[raw::OperationPlan],
) -> Result<(), MirError> {
    let source_values = source.values().collect::<Vec<_>>();
    let slots = check_slots(function, source)?;
    for (claim, original) in function.effects.iter().zip(source.effects()) {
        let mut cursor = Cursor { instructions: &claim.instructions, index: 0 };
        match original.operation() {
            FlowStep::OutputSlot { token, .. } => {
                cursor.expect(&raw::Instruction::ZeroOutput(slots[token]))?;
            }
            FlowStep::PrepareLoan { .. } | FlowStep::Copy { .. } => {
                let stages = match original.preparation() {
                    Some(PrivatePreparation::Loan(loan)) => &loan.stages,
                    Some(PrivatePreparation::Copy(copy)) => &copy.stages,
                    None => return Err(MirError::new("ZRYNA-C4106", "mir-private-plan")),
                };
                for stage in stages {
                    cursor.expect(&raw::Instruction::Storage(*stage))?;
                }
            }
            FlowStep::Reserve { call, maximum_new_owners, live_limit, .. } => {
                require(*live_limit == 64, "ZRYNA-C4105", "mir-shared-instance-limit")?;
                cursor.expect(&raw::Instruction::Reserve {
                    call: *call,
                    maximum: *maximum_new_owners,
                    limit: 64,
                })?;
            }
            step @ FlowStep::Call { .. } => {
                check_call(step, &mut cursor, operations, &source_values)?;
            }
            FlowStep::StatusGuard { call, .. } => {
                cursor.expect(&raw::Instruction::Guard { call: *call })?;
            }
            FlowStep::ReadOutput { call, slot, .. } => {
                cursor.expect(&raw::Instruction::ReadOutput { call: *call, slot: *slot })?;
            }
            FlowStep::Take { owner, .. } => {
                cursor.expect(&raw::Instruction::ValidateTake { owner: *owner })?;
            }
            FlowStep::ConfirmRelease { call, owner, .. } => {
                cursor.expect(&raw::Instruction::ConfirmRelease { call: *call, owner: *owner })?;
            }
            FlowStep::Return { expression, .. } => {
                cursor.expect(&raw::Instruction::Return { expression: *expression })?;
            }
        }
        cursor.expect(&raw::Instruction::CommitOwners(original.completed().to_vec()))?;
        require(
            cursor.index == claim.instructions.len(),
            "ZRYNA-C4106",
            "mir-extra-machine-effect",
        )?;
    }
    Ok(())
}

struct Cursor<'a> {
    instructions: &'a [raw::Instruction],
    index: usize,
}
impl<'a> Cursor<'a> {
    fn next(&mut self) -> Result<&'a raw::Instruction, MirError> {
        let instruction = self
            .instructions
            .get(self.index)
            .ok_or_else(|| MirError::new("ZRYNA-C4106", "mir-missing-machine-effect"))?;
        self.index += 1;
        Ok(instruction)
    }
    fn expect(&mut self, expected: &raw::Instruction) -> Result<(), MirError> {
        require(self.next()? == expected, "ZRYNA-C4106", "mir-machine-order")
    }
}

fn check_slots(
    function: &raw::Function,
    source: VerifiedFunction<'_>,
) -> Result<BTreeMap<usize, OutputSlot>, MirError> {
    let mut slot_index = 0;
    let mut offset = 0_u32;
    let mut slots = BTreeMap::new();
    for effect in source.effects() {
        if let FlowStep::OutputSlot { token, ty, .. } = effect.operation() {
            let width = match ty {
                ValueType::I32Out => 4,
                ValueType::HandleOut | ValueType::BytesOut | ValueType::CountOut => 8,
                _ => return Err(MirError::new("ZRYNA-C4104", "mir-output-category")),
            };
            let padding = (width - offset % width) % width;
            offset = offset
                .checked_add(padding)
                .ok_or_else(|| MirError::new("ZRYNA-C4107", "mir-output-offset"))?;
            let slot = function
                .slots
                .get(slot_index)
                .ok_or_else(|| MirError::new("ZRYNA-C4105", "mir-missing-output-slot"))?;
            let expected = OutputSlot {
                token: *token,
                offset,
                bytes: width,
                alignment: width,
                zeroed: true,
                initialized_at_entry: false,
            };
            require(
                *slot == expected && slots.insert(*token, *slot).is_none(),
                "ZRYNA-C4105",
                "mir-distinct-zeroed-output",
            )?;
            offset = offset
                .checked_add(width)
                .ok_or_else(|| MirError::new("ZRYNA-C4107", "mir-output-size"))?;
            slot_index += 1;
        }
    }
    let frame_padding = (16 - offset % 16) % 16;
    let size = offset
        .checked_add(frame_padding)
        .ok_or_else(|| MirError::new("ZRYNA-C4107", "mir-frame-padding"))?;
    require(
        slot_index == function.slots.len() && function.output_frame_bytes == size,
        "ZRYNA-C4105",
        "mir-exact-output-frame",
    )?;
    Ok(slots)
}

fn check_call(
    step: &FlowStep,
    cursor: &mut Cursor<'_>,
    operations: &[raw::OperationPlan],
    source_values: &[zryna_native_c_ir::VerifiedValue<'_>],
) -> Result<(), MirError> {
    let FlowStep::Call {
        expression,
        call,
        operation,
        entry,
        outputs,
        created_owners,
        recoverable,
        boundary_checks,
        ..
    } = step
    else {
        return Err(MirError::new("ZRYNA-C4106", "mir-call-primitive"));
    };
    for check in boundary_checks {
        if !matches!(check, BoundaryCheck::BooleanResult) {
            cursor.expect(&raw::Instruction::Check(check.clone()))?;
        }
    }
    let raw::Instruction::Invoke {
        call: actual_call,
        operation: actual_operation,
        arguments,
        entry: actual_entry,
    } = cursor.next()?
    else {
        return Err(MirError::new("ZRYNA-C4106", "mir-c-entry-order"));
    };
    let declaration = &operations[*operation].declaration;
    let source_value = source_values
        .get(*expression)
        .ok_or_else(|| MirError::new("ZRYNA-C4106", "mir-call-source"))?;
    let ValueKind::Primitive(primitive, args) = &source_value.definition().kind else {
        return Err(MirError::new("ZRYNA-C4106", "mir-call-primitive"));
    };
    require(
        matches!(primitive, Primitive::RawCall | Primitive::Release)
            && args.len() == declaration.parameters.len() + 1
            && arguments.len() == declaration.parameters.len()
            && actual_call == call
            && actual_operation == operation
            && actual_entry == entry,
        "ZRYNA-C4106",
        "mir-exact-call-binding",
    )?;
    for ((argument, id), parameter) in arguments.iter().zip(&args[1..]).zip(&declaration.parameters)
    {
        let expected = if parameter.abi == AbiType::BytesRelease {
            raw::Operand::OwnerPointer(*id)
        } else {
            raw::Operand::Value(*id)
        };
        require(*argument == expected, "ZRYNA-C4106", "mir-call-argument-order")?;
    }
    if declaration.mode == Mode::Status {
        cursor.expect(&raw::Instruction::ClassifyStatus { call: *call })?;
        cursor.expect(&raw::Instruction::StatusZeroRegister {
            call: *call,
            owners: created_owners.clone(),
        })?;
        cursor.expect(&raw::Instruction::SettleKnownStatusReservation {
            call: *call,
            maximum: created_owners.len(),
            recoverable: recoverable.clone(),
        })?;
        cursor
            .expect(&raw::Instruction::StatusZeroOutputs { call: *call, slots: outputs.clone() })?;
    }
    for check in boundary_checks {
        if matches!(check, BoundaryCheck::BooleanResult) {
            cursor.expect(&raw::Instruction::Check(BoundaryCheck::BooleanResult))?;
        }
    }
    Ok(())
}
