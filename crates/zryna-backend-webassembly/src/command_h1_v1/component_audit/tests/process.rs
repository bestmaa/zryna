//! A valid process import cannot extend the sealed command execution graph.

use wasm_encoder::{
    ComponentImportSection, ComponentSection as _, ComponentTypeRef, ComponentTypeSection,
    InstanceType, PrimitiveValType,
};
use wasmparser::{Validator, WasmFeatures};

use super::Candidate;

#[test]
fn appended_typed_process_import_rejects_before_instantiation() {
    let candidate = Candidate::new("pure-entry");
    let types =
        Validator::new_with_features(WasmFeatures::WASM1.union(WasmFeatures::COMPONENT_MODEL))
            .validate_all(candidate.artifact.bytes())
            .expect("independently valid retained component");
    let index = types.as_ref().component_type_count();

    // The independent interface has the pinned exit-with-code(u8) -> () shape.
    // Appending it leaves both audited core modules and every existing index intact.
    let mut interface = InstanceType::new();
    interface.ty().function().params([("status-code", PrimitiveValType::U8)]).result(None);
    interface.export("exit-with-code", ComponentTypeRef::Func(0));
    let mut added_types = ComponentTypeSection::new();
    added_types.instance(&interface);
    let mut imports = ComponentImportSection::new();
    imports.import("wasi:cli/exit@0.2.12", ComponentTypeRef::Instance(index));

    let mut bytes = candidate.artifact.bytes().to_vec();
    added_types.append_to_component(&mut bytes);
    imports.append_to_component(&mut bytes);
    // This first validates the whole mutant, then checks the sealed final-byte audit.
    // Its recovery check audits the original factory artifact again without an engine.
    candidate.reject_valid(&bytes, "ZRYNA-W4104");
}
