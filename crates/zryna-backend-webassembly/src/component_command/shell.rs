//! Preserve every authenticated world import while connecting the two private core modules.

use wasm_encoder::{
    ComponentBuilder, ComponentExportKind, ComponentExportSection, ComponentInstanceSection,
    ComponentSection, ComponentTypeRef, ComponentValType, ExportKind, ModuleArg,
};
use zryna_diagnostics::Diagnostic;

use crate::{ValidatedWebAssemblyArtifact, wit_world_audit::AuthenticatedCommandWorld};

use super::bridge::{CORE_INSTANCE, RUN_EXPORT};

pub(super) use crate::command_world_types::RUN_INTERFACE;
pub(super) const MAX_COMPONENT_BYTES: usize = 1024 * 1024;

pub(super) fn encode(
    world: &AuthenticatedCommandWorld,
    core: &ValidatedWebAssemblyArtifact,
    bridge: &[u8],
) -> Result<Vec<u8>, Diagnostic> {
    let mut component = ComponentBuilder::default();
    let (run_interface_type, _) = crate::command_world_types::import_world(&mut component, world)?;

    let core_module = component.core_module_raw(None, core.bytes());
    let bridge_module = component.core_module_raw(None, bridge);
    let core_instance = component.core_instantiate(None, core_module, []);
    let bridge_instance = component.core_instantiate(
        None,
        bridge_module,
        [(CORE_INSTANCE, ModuleArg::Instance(core_instance))],
    );
    let core_run = component.core_alias_export(None, bridge_instance, RUN_EXPORT, ExportKind::Func);
    let (result_type, result) = component.type_defined(None);
    result.result(None, None);
    let (run_type, mut function) = component.type_function(None);
    function.params([] as [(&str, ComponentValType); 0]);
    function.result(Some(ComponentValType::Type(result_type)));
    let run = component.lift_func(None, core_run, run_type, []);

    let run_instance = component.instance_count();
    let mut bytes = component.finish();
    let mut instances = ComponentInstanceSection::new();
    instances.export_items([(RUN_EXPORT, ComponentExportKind::Func, run)]);
    instances.append_to_component(&mut bytes);
    let mut exports = ComponentExportSection::new();
    exports.export(
        RUN_INTERFACE,
        ComponentExportKind::Instance,
        run_instance,
        Some(ComponentTypeRef::Instance(run_interface_type)),
    );
    exports.append_to_component(&mut bytes);
    if bytes.len() > MAX_COMPONENT_BYTES {
        return Err(invalid("command component exceeds 1048576 bytes"));
    }
    Ok(bytes)
}

fn invalid(message: &str) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4011",
        None,
        message,
        "restore the authenticated command world and bounded private bridge",
    )
}
