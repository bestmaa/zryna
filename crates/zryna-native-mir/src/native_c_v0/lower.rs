//! Machine candidate production. Independent admission never calls this module.

use super::{
    MirError, VerifiedMirProgram,
    abi::{Lane, Location, OutputSlot, Register, ResultLane, Signature},
    raw, require,
};
use std::collections::BTreeMap;
use zryna_native_c_ir::contract::{Binding, Statement};
use zryna_native_c_ir::{
    VerifiedNativeCProgram,
    contract::{AbiType, BoundaryCheck, FlowStep, Mode, Operation, PrivatePreparation, ValueType},
    raw::ValueKind,
};

/// Produces machine claims and invokes mandatory independent verification.
/// # Errors
/// Rejects unrepresentable machine storage or independent admission failure atomically.
pub fn lower(source: &VerifiedNativeCProgram) -> Result<VerifiedMirProgram, MirError> {
    super::verify(lower_unverified(source)?, source)
}

/// Produces untrusted claims for compiler production and hostile mutation tests.
/// # Errors
/// Rejects checked machine address/size arithmetic or an inconsistent sealed inventory.
pub fn lower_unverified(source: &VerifiedNativeCProgram) -> Result<raw::Program, MirError> {
    preflight_source(source)?;
    let operations = source
        .operations()
        .map(|operation| {
            Ok(raw::OperationPlan {
                index: operation.index(),
                declaration: operation.declaration().clone(),
                signature: signature(operation.declaration())?,
            })
        })
        .collect::<Result<Vec<_>, MirError>>()?;
    let mut functions = Vec::new();
    for (source_function, original) in
        source.functions().zip(source.private_authority().body_authority().functions())
    {
        functions.push(lower_function(
            source_function,
            original.parameters(),
            original.statements(),
            &operations,
        )?);
    }
    Ok(raw::Program {
        source_map: source.source_map_identity(),
        target: source.target().into(),
        storage: zryna_native_c_ir::raw::Storage {
            universe: source.native_layouts().universe_identity().as_bytes(),
            linear: *source.linear_layouts().fingerprint(),
            native: *source.native_layouts().fingerprint(),
            runtime: source.runtime_abi().identifier().into(),
        },
        dispatcher: super::entry::dispatcher(),
        operations,
        functions,
    })
}

fn preflight_source(source: &VerifiedNativeCProgram) -> Result<(), MirError> {
    let mut actions = 0_usize;
    for function in source.functions() {
        for effect in function.effects() {
            // An upper bound for ordinary instructions is checked before candidate allocation.
            // Terminal expansion is linear: one action per loan, two per drop and one finish.
            actions = actions.checked_add(32).ok_or_else(overflow)?;
            for exit in effect.exits() {
                let count = exit
                    .cleanup
                    .len()
                    .checked_mul(2)
                    .and_then(|drops| drops.checked_add(exit.end_loans.len()))
                    .and_then(|prefix| prefix.checked_add(1))
                    .ok_or_else(overflow)?;
                actions = actions.checked_add(count).ok_or_else(overflow)?;
            }
        }
    }
    // Existing sealed source cardinalities bound this checked linear expansion. No new
    // complete-program admission ceiling substitutes for the accepted source budgets.
    Ok(())
}

fn overflow() -> MirError {
    MirError::new("ZRYNA-C4107", "mir-machine-size-overflow")
}
fn align(size: u32, alignment: u32) -> Result<u32, MirError> {
    size.checked_add(alignment - 1).map(|size| size / alignment * alignment).ok_or_else(overflow)
}
fn signature(operation: &Operation) -> Result<Signature, MirError> {
    let registers =
        [Register::Rdi, Register::Rsi, Register::Rdx, Register::Rcx, Register::R8, Register::R9];
    let mut parameters = Vec::new();
    for (index, parameter) in operation.parameters.iter().enumerate() {
        let location = if let Some(register) = registers.get(index) {
            Location::Register(*register)
        } else {
            Location::Stack(
                u32::try_from(index - 6)
                    .map_err(|_| overflow())?
                    .checked_mul(8)
                    .ok_or_else(overflow)?,
            )
        };
        parameters.push(Lane {
            abi: parameter.abi,
            bits: bits(parameter.abi)?,
            location,
            canonical_bool: parameter.abi == AbiType::Bool32,
        });
    }
    let result = if operation.result == AbiType::Unit {
        None
    } else {
        Some(ResultLane {
            abi: operation.result,
            bits: bits(operation.result)?,
            canonical_bool: operation.result == AbiType::Bool32,
        })
    };
    let stack = u32::try_from(parameters.len().saturating_sub(6))
        .map_err(|_| overflow())?
        .checked_mul(8)
        .ok_or_else(overflow)?;
    Ok(Signature { parameters, result, outgoing_bytes: align(stack, 16)?, stack_alignment: 16 })
}
fn bits(ty: AbiType) -> Result<u8, MirError> {
    match ty {
        AbiType::CI32 | AbiType::CInt | AbiType::Bool32 => Ok(32),
        AbiType::Count
        | AbiType::BytesIn
        | AbiType::BytesOwnedOut
        | AbiType::CountOut
        | AbiType::I32Out
        | AbiType::HandleIn
        | AbiType::HandleOut
        | AbiType::BytesRelease => Ok(64),
        AbiType::Unit => Err(MirError::new("ZRYNA-C4104", "mir-unit-argument")),
    }
}

fn lower_function(
    source_function: zryna_native_c_ir::VerifiedFunction<'_>,
    bindings: &[Binding],
    statements: &[Statement],
    operations: &[raw::OperationPlan],
) -> Result<raw::Function, MirError> {
    let mut slots = Vec::new();
    let mut frame = 0_u32;
    let mut slot_indices = BTreeMap::new();
    for effect in source_function.effects() {
        if let FlowStep::OutputSlot { token, ty, .. } = effect.operation() {
            let bytes = if *ty == ValueType::I32Out { 4 } else { 8 };
            frame = align(frame, bytes)?;
            slot_indices.insert(*token, slots.len());
            slots.push(OutputSlot {
                token: *token,
                offset: frame,
                bytes,
                alignment: bytes,
                zeroed: true,
                initialized_at_entry: false,
            });
            frame = frame.checked_add(bytes).ok_or_else(overflow)?;
        }
    }
    let values =
        source_function.values().map(|value| value.definition().clone()).collect::<Vec<_>>();
    let mut effects = Vec::new();
    for effect in source_function.effects() {
        effects.push(machine_effect(effect, operations, &values, &slots, &slot_indices)?);
    }
    let (file, ordinal) = source_function.identity();
    Ok(raw::Function {
        file,
        ordinal,
        span: source_function.span(),
        name: source_function.name().into(),
        bindings: bindings.to_vec(),
        parameters: source_function.parameters().to_vec(),
        result: source_function.result(),
        export: source_function.export(),
        entry: super::entry::private(file.index(), ordinal),
        statements: statements.to_vec(),
        values,
        private_owners: source_function.private_owners().to_vec(),
        slots,
        output_frame_bytes: align(frame, 16)?,
        effects,
    })
}

fn machine_effect(
    effect: zryna_native_c_ir::VerifiedEffect<'_>,
    operations: &[raw::OperationPlan],
    values: &[zryna_native_c_ir::raw::Value],
    slots: &[OutputSlot],
    slot_indices: &BTreeMap<usize, usize>,
) -> Result<raw::Effect, MirError> {
    let mut instructions = Vec::new();
    match effect.operation() {
        FlowStep::OutputSlot { token, .. } => {
            let slot = slot_indices
                .get(token)
                .and_then(|index| slots.get(*index))
                .ok_or_else(|| MirError::new("ZRYNA-C4106", "mir-output-token"))?;
            instructions.push(raw::Instruction::ZeroOutput(*slot));
        }
        FlowStep::PrepareLoan { .. } | FlowStep::Copy { .. } => {
            let stages = match effect.preparation() {
                Some(PrivatePreparation::Loan(loan)) => &loan.stages,
                Some(PrivatePreparation::Copy(copy)) => &copy.stages,
                None => return Err(MirError::new("ZRYNA-C4106", "mir-preparation")),
            };
            instructions.extend(stages.iter().copied().map(raw::Instruction::Storage));
        }
        FlowStep::Reserve { call, maximum_new_owners, live_limit, .. } => {
            instructions.push(raw::Instruction::Reserve {
                call: *call,
                maximum: *maximum_new_owners,
                limit: *live_limit,
            });
        }
        step @ FlowStep::Call { .. } => append_call(step, &mut instructions, operations, values)?,
        FlowStep::StatusGuard { call, .. } => {
            instructions.push(raw::Instruction::Guard { call: *call });
        }
        FlowStep::ReadOutput { call, slot, .. } => {
            instructions.push(raw::Instruction::ReadOutput { call: *call, slot: *slot });
        }
        FlowStep::Take { owner, .. } => {
            instructions.push(raw::Instruction::ValidateTake { owner: *owner });
        }
        FlowStep::ConfirmRelease { call, owner, .. } => {
            instructions.push(raw::Instruction::ConfirmRelease { call: *call, owner: *owner });
        }
        FlowStep::Return { expression, .. } => {
            instructions.push(raw::Instruction::Return { expression: *expression });
        }
    }
    instructions.push(raw::Instruction::CommitOwners(effect.completed().to_vec()));
    let exit_instructions = effect
        .exits()
        .iter()
        .map(|exit| {
            let mut actions = Vec::new();
            actions.extend(exit.end_loans.iter().copied().map(raw::ExitInstruction::EndLoan));
            for (drop, owner) in exit.cleanup.iter().enumerate() {
                actions.push(raw::ExitInstruction::Release { drop, owner: owner.owner() });
                actions.push(raw::ExitInstruction::StopOnReleaseFailure);
            }
            actions.push(raw::ExitInstruction::Finish);
            actions
        })
        .collect();
    Ok(raw::Effect {
        id: effect.id(),
        operation: effect.operation().clone(),
        preparation: effect.preparation().cloned(),
        instructions,
        exits: effect.exits().to_vec(),
        completed: effect.completed().to_vec(),
        exit_instructions,
    })
}

fn append_call(
    step: &FlowStep,
    instructions: &mut Vec<raw::Instruction>,
    operations: &[raw::OperationPlan],
    values: &[zryna_native_c_ir::raw::Value],
) -> Result<(), MirError> {
    let FlowStep::Call {
        expression,
        call,
        operation,
        entry,
        boundary_checks,
        outputs,
        created_owners,
        recoverable,
        ..
    } = step
    else {
        return Err(MirError::new("ZRYNA-C4106", "mir-call-expression"));
    };
    for check in boundary_checks {
        if *check != BoundaryCheck::BooleanResult {
            instructions.push(raw::Instruction::Check(check.clone()));
        }
    }
    let declaration = &operations[*operation].declaration;
    let ValueKind::Primitive(_, args) = &values[*expression].kind else {
        return Err(MirError::new("ZRYNA-C4106", "mir-call-expression"));
    };
    require(args.len() == declaration.parameters.len() + 1, "ZRYNA-C4106", "mir-call-arguments")?;
    let arguments = args[1..]
        .iter()
        .zip(&declaration.parameters)
        .map(|(id, parameter)| {
            if parameter.abi == AbiType::BytesRelease {
                raw::Operand::OwnerPointer(*id)
            } else {
                raw::Operand::Value(*id)
            }
        })
        .collect();
    instructions.push(raw::Instruction::Invoke {
        call: *call,
        operation: *operation,
        arguments,
        entry: *entry,
    });
    if declaration.mode == Mode::Status {
        instructions.push(raw::Instruction::ClassifyStatus { call: *call });
        instructions.push(raw::Instruction::StatusZeroRegister {
            call: *call,
            owners: created_owners.clone(),
        });
        instructions.push(raw::Instruction::SettleKnownStatusReservation {
            call: *call,
            maximum: created_owners.len(),
            recoverable: recoverable.clone(),
        });
        instructions
            .push(raw::Instruction::StatusZeroOutputs { call: *call, slots: outputs.clone() });
    }
    if boundary_checks.contains(&BoundaryCheck::BooleanResult) {
        instructions.push(raw::Instruction::Check(BoundaryCheck::BooleanResult));
    }
    Ok(())
}
