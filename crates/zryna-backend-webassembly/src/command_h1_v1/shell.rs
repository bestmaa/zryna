use crate::{command_world_types, wit_world_audit::AuthenticatedCommandWorld};
use wasm_encoder::{
    CanonicalOption, ComponentBuilder, ComponentExportKind, ComponentExportSection,
    ComponentInstanceSection, ComponentSection, ComponentTypeRef, ComponentValType, ExportKind,
    ModuleArg,
};
use zryna_diagnostics::Diagnostic;

pub(super) fn encode(
    world: &AuthenticatedCommandWorld,
    storage: &[u8],
    language: &[u8],
) -> Result<Vec<u8>, Diagnostic> {
    let mut component = ComponentBuilder::default();
    let (run_interface_type, environment) =
        command_world_types::import_world(&mut component, world)?;
    let environment =
        component.alias_export(environment, "get-environment", ComponentExportKind::Func);
    let storage_module = component.core_module_raw(None, storage);
    let language_module = component.core_module_raw(None, language);
    let storage = component.core_instantiate(None, storage_module, []);
    let memory = component.core_alias_export(None, storage, "memory", ExportKind::Memory);
    let realloc = component.core_alias_export(None, storage, "realloc", ExportKind::Func);
    let lowered = component.lower_func(
        None,
        environment,
        [CanonicalOption::UTF8, CanonicalOption::Memory(memory), CanonicalOption::Realloc(realloc)],
    );
    let host =
        component.core_instantiate_exports(None, [("get-environment", ExportKind::Func, lowered)]);
    let language = component.core_instantiate(
        None,
        language_module,
        [("storage", ModuleArg::Instance(storage)), ("host", ModuleArg::Instance(host))],
    );
    let core_run = component.core_alias_export(None, language, "run", ExportKind::Func);
    let (result_type, result) = component.type_defined(None);
    result.result(None, None);
    let (run_type, mut function) = component.type_function(None);
    function.params([] as [(&str, ComponentValType); 0]);
    function.result(Some(ComponentValType::Type(result_type)));
    let run = component.lift_func(None, core_run, run_type, []);
    let run_instance = component.instance_count();
    let mut bytes = component.finish();
    let mut instances = ComponentInstanceSection::new();
    instances.export_items([("run", ComponentExportKind::Func, run)]);
    instances.append_to_component(&mut bytes);
    let mut exports = ComponentExportSection::new();
    exports.export(
        command_world_types::RUN_INTERFACE,
        ComponentExportKind::Instance,
        run_instance,
        Some(ComponentTypeRef::Instance(run_interface_type)),
    );
    exports.append_to_component(&mut bytes);
    if bytes.len() > 1_048_576 {
        return Err(Diagnostic::error(
            "ZRYNA-W4102",
            None,
            "command component exceeds 1048576 bytes",
            "reduce the verified command within its component budget",
        ));
    }
    Ok(bytes)
}
