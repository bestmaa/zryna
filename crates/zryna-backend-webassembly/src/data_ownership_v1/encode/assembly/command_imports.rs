use super::index_error;
use wasm_encoder::{EntityType, GlobalType, ImportSection, MemoryType, Module, ValType};

pub(super) fn encode(
    module: &mut Module,
    arities: &[usize],
    callback_type: u32,
    copy_type: u32,
    drain_type: u32,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    let validate_type =
        u32::try_from(arities.iter().position(|arity| *arity == 3).ok_or_else(index_error)?)
            .map_err(|_| index_error())?;
    let mut imports = ImportSection::new();
    for (namespace, name, ty) in [
        ("storage", "allocate", 0),
        ("storage", "copy", copy_type),
        ("host", "get-environment", callback_type),
        ("storage", "validate", validate_type),
        ("storage", "drain", drain_type),
        ("storage", "canonical-state", 0),
    ] {
        imports.import(namespace, name, EntityType::Function(ty));
    }
    imports.import(
        "storage",
        "memory",
        EntityType::Memory(MemoryType {
            minimum: 256,
            maximum: Some(256),
            memory64: false,
            shared: false,
            page_size_log2: None,
        }),
    );
    for name in ["arena", "status", "drops", "live", "peak", "references"] {
        imports.import(
            "storage",
            name,
            EntityType::Global(GlobalType { val_type: ValType::I32, mutable: true, shared: false }),
        );
    }
    module.section(&imports);
    Ok(())
}
