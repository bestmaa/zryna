//! Manually authored reviewed instruction references, with four closed authority substitutions.

use zryna_diagnostics::Diagnostic;
use zryna_ir::{
    command_h1_v1::VerifiedProgram,
    data_ownership_v1::{VerifiedBackendInstruction, VerifiedBlock, VerifiedModule},
};
use zryna_layout::TypeCategory;

use super::invalid;

const BRIDGE: &str = include_str!("reference/bridge.txt");
const KEY_BYTE: &str = include_str!("reference/key-byte.txt");

pub(super) fn derive(program: &VerifiedProgram) -> Result<Vec<String>, Diagnostic> {
    let requirement = program.source().environment().ok_or_else(invalid)?;
    let effects = program
        .modules()
        .flat_map(VerifiedModule::functions)
        .flat_map(zryna_ir::data_ownership_v1::VerifiedFunction::blocks)
        .flat_map(VerifiedBlock::instructions)
        .filter_map(|instruction| match instruction.backend_instruction() {
            VerifiedBackendInstruction::EnvironmentLookup(key) => {
                Some((key, instruction.result_type()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(key, Some(ty))] = effects.as_slice() else {
        return Err(invalid());
    };
    if *key != requirement.key() || key.is_empty() || key.len() > 64 {
        return Err(invalid());
    }
    let layouts = program.linear32_layouts();
    let outcome = layouts.type_by_id(*ty).ok_or_else(invalid)?;
    let [found, missing] = outcome.variants() else {
        return Err(invalid());
    };
    let string = layouts.type_by_id(found.payload().ok_or_else(invalid)?).ok_or_else(invalid)?;
    let (offset, payload_size) = outcome.enum_payload_layout().ok_or_else(invalid)?;
    if outcome.category() != TypeCategory::Enum
        || found.ordinal() != 0
        || missing.ordinal() != 1
        || missing.payload().is_some()
        || string.category() != TypeCategory::String
        || string.size() != 12
        || string.alignment() != 4
        || payload_size != string.size()
        || outcome.alignment() != 4
        || offset < 4
        || outcome.size() < offset + payload_size
    {
        return Err(invalid());
    }
    let size = i32::try_from(outcome.size()).map_err(|_| invalid())?;
    let offset = i32::try_from(offset).map_err(|_| invalid())?;
    let length = i32::try_from(key.len()).map_err(|_| invalid())?;
    let mut expected = Vec::new();
    for line in lines(BRIDGE) {
        match line {
            "i32.const @outcome_size" => expected.push(format!("i32.const {size}")),
            "i32.const @payload_offset" => expected.push(format!("i32.const {offset}")),
            "i32.const @key_length" => expected.push(format!("i32.const {length}")),
            "@key_bytes" => {
                for (position, byte) in key.bytes().enumerate() {
                    for instruction in lines(KEY_BYTE) {
                        expected.push(match instruction {
                            "i32.load8_u memory=0 align=0 offset=@key_offset" => {
                                format!("i32.load8_u memory=0 align=0 offset={position}")
                            }
                            "i32.const @key_byte" => format!("i32.const {byte}"),
                            _ if instruction.contains('@') => return Err(invalid()),
                            _ => instruction.to_owned(),
                        });
                    }
                }
            }
            _ if line.contains('@') => return Err(invalid()),
            _ => expected.push(line.to_owned()),
        }
    }
    Ok(expected)
}

fn lines(reference: &str) -> impl Iterator<Item = &str> {
    reference.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#'))
}
