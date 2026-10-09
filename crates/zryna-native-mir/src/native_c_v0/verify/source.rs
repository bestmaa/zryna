//! Exact retained source and issuer binding, before inspecting machine-controlled payloads.

use super::super::{MirError, raw, require};
use zryna_native_c_ir::{
    VerifiedNativeCProgram,
    contract::{AbiType, Direction, Mode},
};

pub(super) fn check(
    program: &raw::Program,
    source: &VerifiedNativeCProgram,
) -> Result<(), MirError> {
    require(
        program.source_map == source.source_map_identity(),
        "ZRYNA-C4102",
        "mir-original-issuer",
    )?;
    require(
        program.target == source.target() && program.target == "x86_64-unknown-linux-gnu",
        "ZRYNA-C4103",
        "mir-target",
    )?;
    require(
        program.storage.universe == source.native_layouts().universe_identity().as_bytes()
            && program.storage.linear == *source.linear_layouts().fingerprint()
            && program.storage.native == *source.native_layouts().fingerprint()
            && program.storage.runtime == source.runtime_abi().identifier(),
        "ZRYNA-C4102",
        "mir-retained-layout-runtime",
    )?;
    require(
        program.operations.len() == source.operations().len()
            && program.functions.len() == source.functions().len(),
        "ZRYNA-C4102",
        "mir-complete-inventory",
    )?;
    for (claim, original) in program.operations.iter().zip(source.operations()) {
        // Equality checks vector/string lengths before content; all equal payloads inherit the
        // sealed IR's bounds. No raw declaration is decoded, expanded or trusted as an issuer.
        require(
            claim.index == original.index() && claim.declaration == *original.declaration(),
            "ZRYNA-C4102",
            "mir-exact-declaration",
        )?;
    }
    for ((claim, original), body) in program
        .functions
        .iter()
        .zip(source.functions())
        .zip(source.private_authority().body_authority().functions())
    {
        require(
            (claim.file, claim.ordinal) == original.identity()
                && claim.span == original.span()
                && claim.name == original.name()
                && claim.parameters == original.parameters()
                && claim.bindings == body.parameters()
                && claim.result == original.result()
                && claim.export == original.export()
                && claim.statements == body.statements()
                && claim.private_owners == original.private_owners()
                && claim.values.len() == original.values().len()
                && claim.effects.len() == original.effects().len(),
            "ZRYNA-C4106",
            "mir-complete-original-function",
        )?;
        for (value, original) in claim.values.iter().zip(original.values()) {
            let original = original.definition();
            require(
                value.id == original.id
                    && value.span == original.span
                    && value.ty == original.ty
                    && value.kind == original.kind
                    && value.token == original.token
                    && value.status_call == original.status_call
                    && value.origin == original.origin,
                "ZRYNA-C4106",
                "mir-source-value-provenance",
            )?;
        }
        for (effect, original) in claim.effects.iter().zip(original.effects()) {
            require(
                effect.id == original.id()
                    && &effect.operation == original.operation()
                    && effect.preparation.as_ref() == original.preparation()
                    && effect.exits == original.exits()
                    && effect.completed == original.completed(),
                "ZRYNA-C4106",
                "mir-original-effect-exits",
            )?;
        }
        if let Some(export) = claim.export {
            let operation = &program.operations[export].declaration;
            require(
                operation.direction == Direction::Export
                    && operation.mode == Mode::Direct
                    && operation.resources.is_empty()
                    && matches!(operation.result, AbiType::CI32 | AbiType::CInt | AbiType::Bool32)
                    && operation
                        .parameters
                        .iter()
                        .all(|p| matches!(p.abi, AbiType::CI32 | AbiType::CInt | AbiType::Bool32))
                    && claim.private_owners.is_empty(),
                "ZRYNA-C4104",
                "mir-total-scalar-export",
            )?;
        }
    }
    Ok(())
}
