//! Independent physical C ABI derivation from sealed declarations, never lowering output.

use super::super::{
    MirError,
    abi::{Location, Register, ResultLane},
    raw, require,
};
use zryna_native_c_ir::{VerifiedNativeCProgram, contract::AbiType};

pub(super) fn check(
    program: &raw::Program,
    source: &VerifiedNativeCProgram,
) -> Result<(), MirError> {
    for (claim, original) in program.operations.iter().zip(source.operations()) {
        let declaration = original.declaration();
        require(
            claim.signature.parameters.len() == declaration.parameters.len()
                && claim.signature.stack_alignment == 16,
            "ZRYNA-C4104",
            "mir-calling-convention",
        )?;
        for (index, (lane, parameter)) in
            claim.signature.parameters.iter().zip(&declaration.parameters).enumerate()
        {
            let location = match index {
                0 => Location::Register(Register::Rdi),
                1 => Location::Register(Register::Rsi),
                2 => Location::Register(Register::Rdx),
                3 => Location::Register(Register::Rcx),
                4 => Location::Register(Register::R8),
                5 => Location::Register(Register::R9),
                _ => Location::Stack(
                    u32::try_from(index - 6)
                        .map_err(|_| MirError::new("ZRYNA-C4107", "mir-stack-index"))?
                        * 8,
                ),
            };
            let bits = match parameter.abi {
                AbiType::CI32 | AbiType::CInt | AbiType::Bool32 => 32,
                AbiType::Count
                | AbiType::BytesIn
                | AbiType::BytesOwnedOut
                | AbiType::CountOut
                | AbiType::I32Out
                | AbiType::HandleIn
                | AbiType::HandleOut
                | AbiType::BytesRelease => 64,
                AbiType::Unit => return Err(MirError::new("ZRYNA-C4104", "mir-unit-carrier")),
            };
            require(
                lane.abi == parameter.abi
                    && lane.bits == bits
                    && lane.location == location
                    && lane.canonical_bool == (parameter.abi == AbiType::Bool32),
                "ZRYNA-C4104",
                "mir-exact-integer-lane",
            )?;
        }
        let stack_slots = declaration.parameters.len().saturating_sub(6);
        let stack_bytes = u32::try_from(stack_slots)
            .map_err(|_| MirError::new("ZRYNA-C4107", "mir-stack-size"))?
            * 8;
        let padded = stack_bytes
            .checked_add(15)
            .ok_or_else(|| MirError::new("ZRYNA-C4107", "mir-stack-padding"))?
            & !15;
        require(
            claim.signature.outgoing_bytes == padded,
            "ZRYNA-C4104",
            "mir-outgoing-stack-alignment",
        )?;
        let expected = match declaration.result {
            AbiType::Unit => None,
            AbiType::CI32 | AbiType::CInt | AbiType::Bool32 => Some(ResultLane {
                abi: declaration.result,
                bits: 32,
                canonical_bool: declaration.result == AbiType::Bool32,
            }),
            _ => return Err(MirError::new("ZRYNA-C4104", "mir-result-lane")),
        };
        require(claim.signature.result == expected, "ZRYNA-C4104", "mir-rax-low-width-result")?;
    }
    Ok(())
}
