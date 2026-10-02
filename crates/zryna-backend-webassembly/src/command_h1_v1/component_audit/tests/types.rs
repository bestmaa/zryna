use wasm_encoder::{
    CanonicalFunctionSection, ComponentExportKind, ComponentExportSection,
    ComponentInstanceSection, ComponentTypeRef, ComponentTypeSection, ComponentValType,
    InstanceType, PrimitiveValType,
};
use wasmparser::{CanonicalFunction, Parser, Payload};

use super::{Candidate, range, replace, rewrite};

#[test]
fn absent_public_ascription_and_extra_instance_export_reject_valid_components() {
    let candidate = Candidate::new("environment-match");
    let export = Parser::new(0)
        .parse_all(candidate.artifact.bytes())
        .find_map(|payload| {
            if let Payload::ComponentExportSection(exports) =
                payload.expect("component export syntax")
            {
                Some(exports.range())
            } else {
                None
            }
        })
        .expect("public run interface");
    let mut exports = ComponentExportSection::new();
    exports.export("wasi:cli/run@0.2.12", ComponentExportKind::Instance, 16, None);
    candidate.reject_valid(&replace(candidate.artifact.bytes(), export, &exports), "ZRYNA-W4104");

    let at = Parser::new(0)
        .parse_all(candidate.artifact.bytes())
        .find_map(|payload| {
            if let Payload::ComponentInstanceSection(instances) =
                payload.expect("component instance syntax")
            {
                Some(instances.range())
            } else {
                None
            }
        })
        .expect("public FromExports instance");
    let mut instances = ComponentInstanceSection::new();
    instances.export_items([
        ("run", ComponentExportKind::Func, 1),
        ("extra", ComponentExportKind::Func, 1),
    ]);
    candidate.reject_valid(&replace(candidate.artifact.bytes(), at, &instances), "ZRYNA-W4104");
}

#[test]
fn same_core_carrier_with_changed_public_bool_result_rejects_authenticated_world() {
    let candidate = Candidate::new("pure-entry");
    let (at, original_type) = Parser::new(0)
        .parse_all(candidate.artifact.bytes())
        .find_map(|payload| {
            if let Payload::ComponentCanonicalSection(functions) =
                payload.expect("canonical syntax")
            {
                let at = functions.range();
                for function in functions {
                    if let CanonicalFunction::Lift { type_index, .. } =
                        function.expect("canonical lift")
                    {
                        return Some((at, type_index));
                    }
                }
            }
            None
        })
        .expect("actual run lift type");
    let at = range(at);
    // The original final type is the run function. Independently append a bool function
    // and its matching one-export interface, preserving the same core i32 carrier.
    let function = original_type.checked_add(1).expect("bounded new function type");
    let instance = function.checked_add(1).expect("bounded new interface type");
    let mut types = ComponentTypeSection::new();
    types
        .function()
        .params([] as [(&str, ComponentValType); 0])
        .result(Some(PrimitiveValType::Bool.into()));
    let mut interface = InstanceType::new();
    interface
        .ty()
        .function()
        .params([] as [(&str, ComponentValType); 0])
        .result(Some(PrimitiveValType::Bool.into()));
    interface.export("run", ComponentTypeRef::Func(0));
    types.instance(&interface);
    let mut lift = CanonicalFunctionSection::new();
    lift.lift(2, function, []);
    let mut exports = ComponentExportSection::new();
    exports.export(
        "wasi:cli/run@0.2.12",
        ComponentExportKind::Instance,
        16,
        Some(ComponentTypeRef::Instance(instance)),
    );
    let mut changed_lift = false;
    let mut changed_export = false;
    let bytes = rewrite(candidate.artifact.bytes(), |chunk, component| {
        if chunk.range == at {
            component.section(&types);
            component.section(&lift);
            changed_lift = true;
            true
        } else if chunk.id == 11 {
            component.section(&exports);
            changed_export = true;
            true
        } else {
            false
        }
    });
    assert!(changed_lift && changed_export);
    candidate.reject_valid(&bytes, "ZRYNA-W4012");
}

#[test]
fn public_interface_name_version_changes_reject_valid_components() {
    let candidate = Candidate::new("environment-match");
    let (at, ty) = Parser::new(0)
        .parse_all(candidate.artifact.bytes())
        .find_map(|payload| {
            if let Payload::ComponentExportSection(exports) = payload.expect("export syntax") {
                let at = exports.range();
                let export = exports.into_iter().next().expect("one export").expect("export");
                if let Some(wasmparser::ComponentTypeRef::Instance(ty)) = export.ty {
                    return Some((at, ty));
                }
            }
            None
        })
        .expect("actual run ascription");
    for name in ["wasi:cli/run@0.2.13", "wasi:cli/other@0.2.12"] {
        let mut exports = ComponentExportSection::new();
        exports.export(
            name,
            ComponentExportKind::Instance,
            16,
            Some(ComponentTypeRef::Instance(ty)),
        );
        candidate.reject_valid(
            &replace(candidate.artifact.bytes(), at.clone(), &exports),
            "ZRYNA-W4104",
        );
    }
}
