//! `SysV` INTEGER lane assignment and checked outgoing stack sizing.

use super::{MirError, align, overflow};
use crate::native_c_v0::abi::{Lane, Location, Register, ResultLane, Signature};
use zryna_native_c_ir::contract::{AbiType, Operation};

pub(super) fn signature(operation: &Operation) -> Result<Signature, MirError> {
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
