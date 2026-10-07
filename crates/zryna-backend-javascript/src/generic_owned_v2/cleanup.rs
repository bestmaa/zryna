//! Concrete recursive destruction selects only the active sealed payload.
use super::{Writer, internal};
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::{
    owned_v2::{VerifiedOwnedProgram, raw::Step},
    raw,
};
use zryna_layout::{StorageTarget, TypeCategory};

pub(super) fn declarations(w: &mut Writer, p: &VerifiedOwnedProgram<'_>) -> Result<(), Diagnostic> {
    for (index, ty) in p.layouts(StorageTarget::Linear32V1).types().enumerate() {
        if ty.drop_kind() == 0 {
            continue;
        }
        w.line(format_args!("function $drop{index}($value) {{"))?;
        match ty.category() {
            TypeCategory::String => w.text("$free($value);\n")?,
            TypeCategory::Enum => {
                w.text("switch ($value.tag) {\n")?;
                for (ordinal, (_, payload)) in ty.variants().enumerate() {
                    w.line(format_args!("case {ordinal}:"))?;
                    if let Some(payload) = payload {
                        let child = p
                            .layouts(StorageTarget::Linear32V1)
                            .types()
                            .nth(payload.index() as usize)
                            .ok_or_else(internal)?;
                        if child.drop_kind() != 0 {
                            w.line(format_args!("$drop{}($value.payload);", payload.index()))?;
                        }
                    }
                    w.text("return;\n")?;
                }
                w.text("default: throw new Error('ZRYNA-RT-TAG');\n}\n")?;
            }
            _ => return Err(internal()),
        }
        w.text("}\n")?;
    }
    Ok(())
}
pub(super) fn value_type(f: &raw::Function, id: u32) -> Result<raw::Type, Diagnostic> {
    f.blocks
        .iter()
        .flat_map(|b| b.parameters.iter().chain(b.instructions.iter().map(|i| &i.result)))
        .find(|d| d.id == id)
        .map(|d| d.ty)
        .ok_or_else(internal)
}
pub(super) fn step(
    w: &mut Writer,
    _p: &VerifiedOwnedProgram<'_>,
    f: &raw::Function,
    step: &Step,
) -> Result<(), Diagnostic> {
    for id in &step.cleanup {
        let raw::Type::Stored(index) = value_type(f, *id)? else {
            return Err(internal());
        };
        w.line(format_args!("$drop{index}($v{id});"))?;
    }
    Ok(())
}
