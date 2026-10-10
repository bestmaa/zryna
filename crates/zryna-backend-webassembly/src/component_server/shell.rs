//! Preserve exact world metadata without pruning imports or merging WASI versions.

use std::collections::BTreeMap;

use wasm_encoder::{
    CanonicalOption, ComponentBuilder, ComponentExportKind, ComponentExportSection,
    ComponentInstanceSection, ComponentSection, ComponentTypeRef, ComponentValType, ExportKind,
    ModuleArg,
    reencode::{ReencodeComponent, RoundtripReencoder},
};
use wasmparser::{ComponentType, ComponentTypeDeclaration, Parser, Payload};
use zryna_diagnostics::Diagnostic;

use super::{
    ServerOperation, bridge, invalid, memory,
    world::{CLOCK, HANDLER, HTTP, ID, OPERATIONS, World},
};
use crate::ValidatedWebAssemblyArtifact;

pub(super) const MAX_BYTES: usize = 1024 * 1024;

pub(super) fn encode(
    world: &World,
    core: &ValidatedWebAssemblyArtifact,
    export: &str,
    operation: ServerOperation,
) -> Result<Vec<u8>, Diagnostic> {
    if core.bytes().len() > MAX_BYTES {
        return Err(invalid("server scalar core exceeds the component byte envelope"));
    }
    let metadata = wit_component::metadata::encode(
        world.resolve(),
        world.world(),
        wit_component::StringEncoding::UTF8,
        None,
    )
    .map_err(|_| invalid("authenticated server world metadata cannot be encoded"))?;
    let declarations = declarations(&metadata)?;
    let mut component = ComponentBuilder::default();
    let (imports, handler_type) = import_world(&mut component, declarations)?;
    let http = *imports.get(HTTP).ok_or_else(|| invalid("HTTP types import is missing"))?;
    let request_type = component.alias_export(http, "incoming-request", ComponentExportKind::Type);
    let outparam_type =
        component.alias_export(http, "response-outparam", ComponentExportKind::Type);
    let core_module = component.core_module_raw(None, core.bytes());
    let memory_module = component.core_module_raw(None, &memory::encode());
    let bridge_module = component.core_module_raw(None, &bridge::encode(world, export, operation)?);
    let core_instance = component.core_instantiate(None, core_module, []);
    let memory_instance = component.core_instantiate(None, memory_module, []);
    let memory = component.core_alias_export(None, memory_instance, "memory", ExportKind::Memory);
    let realloc = component.core_alias_export(None, memory_instance, "realloc", ExportKind::Func);
    let mut bindings = Vec::new();
    for name in OPERATIONS {
        let function = component.alias_export(http, name, ComponentExportKind::Func);
        let options = match name {
            "[method]outgoing-response.body" | "[static]response-outparam.set" => {
                vec![CanonicalOption::Memory(memory)]
            }
            "[static]outgoing-body.finish" => vec![
                CanonicalOption::Memory(memory),
                CanonicalOption::Realloc(realloc),
                CanonicalOption::UTF8,
            ],
            _ => vec![],
        };
        let lowered = component.lower_func(None, function, options);
        bindings.push((name, ExportKind::Func, lowered));
    }
    let drop_request = component.resource_drop(request_type);
    bindings.push((bridge::DROP_REQUEST, ExportKind::Func, drop_request));
    if operation == ServerOperation::ClockRead {
        let clock =
            *imports.get(CLOCK).ok_or_else(|| invalid("monotonic clock import is missing"))?;
        let now = component.alias_export(clock, "now", ComponentExportKind::Func);
        let lowered = component.lower_func(None, now, []);
        bindings.push(("clock-now", ExportKind::Func, lowered));
    }
    let host_instance = component.core_instantiate_exports(None, bindings);
    let bridge_instance = component.core_instantiate(
        None,
        bridge_module,
        [
            (bridge::HOST, ModuleArg::Instance(host_instance)),
            (bridge::SCALAR, ModuleArg::Instance(core_instance)),
            (bridge::MEMORY, ModuleArg::Instance(memory_instance)),
        ],
    );
    let handle_core =
        component.core_alias_export(None, bridge_instance, "handle", ExportKind::Func);
    export_handler(component, handler_type, request_type, outparam_type, handle_core)
}

fn export_handler(
    mut component: ComponentBuilder,
    handler_type: u32,
    request_type: u32,
    outparam_type: u32,
    handle_core: u32,
) -> Result<Vec<u8>, Diagnostic> {
    let (request, request_encoder) = component.type_defined(None);
    request_encoder.own(request_type);
    let (outparam, outparam_encoder) = component.type_defined(None);
    outparam_encoder.own(outparam_type);
    let (function_type, mut handle_encoder) = component.type_function(None);
    handle_encoder.params([
        ("request", ComponentValType::Type(request)),
        ("response-out", ComponentValType::Type(outparam)),
    ]);
    handle_encoder.result(None);
    let handle = component.lift_func(None, handle_core, function_type, []);
    let handler_instance = component.instance_count();
    let mut bytes = component.finish();
    let mut instances = ComponentInstanceSection::new();
    instances.export_items([
        ("incoming-request", ComponentExportKind::Type, request_type),
        ("response-outparam", ComponentExportKind::Type, outparam_type),
        ("handle", ComponentExportKind::Func, handle),
    ]);
    instances.append_to_component(&mut bytes);
    let mut exports = ComponentExportSection::new();
    exports.export(
        HANDLER,
        ComponentExportKind::Instance,
        handler_instance,
        Some(ComponentTypeRef::Instance(handler_type)),
    );
    exports.append_to_component(&mut bytes);
    if bytes.len() > MAX_BYTES {
        return Err(invalid("server component exceeds 1048576 bytes"));
    }
    Ok(bytes)
}

fn declarations(bytes: &[u8]) -> Result<Box<[ComponentTypeDeclaration<'_>]>, Diagnostic> {
    let mut world = None;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ComponentTypeSection(types) =
            payload.map_err(|_| invalid("server metadata is malformed"))?
        {
            for ty in types {
                let ComponentType::Component(wrapper) =
                    ty.map_err(|_| invalid("server metadata wrapper is malformed"))?
                else {
                    return Err(invalid("server metadata wrapper is not a component type"));
                };
                let mut wrapper = Vec::from(wrapper).into_iter();
                let Some(ComponentTypeDeclaration::Type(ComponentType::Component(declarations))) =
                    wrapper.next()
                else {
                    return Err(invalid("server metadata body differs"));
                };
                match wrapper.next() {
                    Some(ComponentTypeDeclaration::Export { name, ty })
                        if name.name == ID && ty == wasmparser::ComponentTypeRef::Component(0) => {}
                    _ => return Err(invalid("server metadata identity differs")),
                }
                if wrapper.next().is_some() || world.replace(declarations).is_some() {
                    return Err(invalid("server metadata contains extra worlds"));
                }
            }
        }
    }
    world.ok_or_else(|| invalid("server world metadata is missing"))
}

fn import_world(
    component: &mut ComponentBuilder,
    declarations: Box<[ComponentTypeDeclaration<'_>]>,
) -> Result<(BTreeMap<String, u32>, u32), Diagnostic> {
    let mut reencoder = RoundtripReencoder;
    let mut imports = BTreeMap::new();
    let mut handler = None;
    for declaration in Vec::from(declarations) {
        match declaration {
            ComponentTypeDeclaration::Type(ty) => {
                let (_, encoder) = component.ty(None);
                reencoder
                    .parse_component_type(encoder, ty)
                    .map_err(|_| invalid("server world type reencoding failed"))?;
            }
            ComponentTypeDeclaration::Alias(alias) => {
                component.alias(
                    None,
                    reencoder
                        .component_alias(alias)
                        .map_err(|_| invalid("server world alias reencoding failed"))?,
                );
            }
            ComponentTypeDeclaration::Import(import) => {
                let ty = reencoder
                    .component_type_ref(import.ty)
                    .map_err(|_| invalid("server world import reencoding failed"))?;
                if !matches!(ty, ComponentTypeRef::Instance(_)) {
                    return Err(invalid("server world contains a non-interface import"));
                }
                let instance = component.import(import.name, ty);
                if imports.insert(import.name.name.to_owned(), instance).is_some() {
                    return Err(invalid("server world duplicates an import"));
                }
            }
            ComponentTypeDeclaration::Export { name, ty } => match ty {
                wasmparser::ComponentTypeRef::Instance(index)
                    if name.name == HANDLER && handler.replace(index).is_none() => {}
                _ => return Err(invalid("server world contains an unexpected export")),
            },
            ComponentTypeDeclaration::CoreType(_) => {
                return Err(invalid("server world contains an unexpected core type"));
            }
        }
    }
    if imports.len() != 8 {
        return Err(invalid("server world must retain eight resolved imports"));
    }
    Ok((imports, handler.ok_or_else(|| invalid("server handler interface is missing"))?))
}
