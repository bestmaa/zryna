//! Exact storage-first execution graph and unchanged authenticated public WIT world.

use crate::{
    component_command::{
        type_budget::{self, TypeBudget},
        type_graph,
    },
    wit_world_audit::AuthenticatedCommandWorld,
};
use std::ops::Range;
use wasmparser::{
    CanonicalFunction, CanonicalOption, ComponentAlias, ComponentExternalKind, ComponentInstance,
    ComponentTypeRef, Encoding, ExternalKind, Instance, Parser, Payload, Validator, WasmFeatures,
};
use wit_parser::decoding::DecodedWasm;
use zryna_diagnostics::Diagnostic;
mod graph_budget;
#[cfg(test)]
mod tests;

pub(super) fn audit(
    bytes: &[u8],
    storage: &[u8],
    language: &[u8],
    world: &AuthenticatedCommandWorld,
) -> Result<u64, Diagnostic> {
    if bytes.len() > 1_048_576 {
        return Err(invalid());
    }
    let mut graph = Graph::default();
    for payload in Parser::new(0).parse_all(bytes) {
        graph.payload(bytes, payload.map_err(|_| invalid())?)?;
    }
    if graph.phase != 11
        || graph.inside_core
        || graph.imports != 16
        || graph.modules.as_slice() != [storage, language]
    {
        return Err(invalid());
    }
    Validator::new_with_features(WasmFeatures::WASM1.union(WasmFeatures::COMPONENT_MODEL))
        .validate_all(bytes)
        .map_err(|_| invalid())?;
    let DecodedWasm::Component(resolve, id) =
        wit_parser::decoding::decode(bytes).map_err(|_| invalid())?
    else {
        return Err(invalid());
    };
    type_graph::compare(world, &resolve, id)?;
    graph.language_base.ok_or_else(invalid)
}

#[derive(Default)]
struct Graph<'a> {
    modules: Vec<&'a [u8]>,
    language_base: Option<u64>,
    inside_core: bool,
    imports: u32,
    environment: Option<u32>,
    phase: u8,
    sections: usize,
    aliases: u32,
    budget: TypeBudget,
}

impl<'a> Graph<'a> {
    fn payload(&mut self, bytes: &'a [u8], payload: Payload<'a>) -> Result<(), Diagnostic> {
        if self.inside_core {
            if matches!(payload, Payload::End(_)) {
                self.inside_core = false;
            }
            return Ok(());
        }
        self.sections += 1;
        if self.sections > 256 {
            return Err(invalid());
        }
        match payload {
            Payload::Version { encoding: Encoding::Component, .. } | Payload::End(_) => {}
            Payload::ComponentTypeSection(types) => {
                let range = types.range();
                self.budget.section(slice(bytes, range.clone())?, range.start)?;
            }
            Payload::ComponentImportSection(imports) if self.phase == 0 => {
                if imports.count() > 16 - self.imports {
                    return Err(invalid());
                }
                for import in imports {
                    let import = import.map_err(|_| invalid())?;
                    type_budget::external_name(import.name)?;
                    let ComponentTypeRef::Instance(index) = import.ty else {
                        return Err(invalid());
                    };
                    self.budget.interface_use(index, true)?;
                    if import.name.name == "wasi:cli/environment@0.2.12"
                        && self.environment.replace(self.imports).is_some()
                    {
                        return Err(invalid());
                    }
                    self.imports += 1;
                }
            }
            Payload::ComponentAliasSection(aliases) => {
                if aliases.count() > 4096 - self.aliases {
                    return Err(invalid());
                }
                self.aliases += aliases.count();
                for alias in aliases {
                    self.alias(&alias.map_err(|_| invalid())?)?;
                }
            }
            Payload::ModuleSection { unchecked_range, .. }
                if self.phase == 1 && self.imports == 16 && self.modules.len() < 2 =>
            {
                if self.modules.len() == 1 {
                    self.language_base = Some(unchecked_range.start);
                }
                self.modules.push(slice(bytes, unchecked_range)?);
                self.inside_core = true;
            }
            Payload::InstanceSection(instances) if self.modules.len() == 2 => {
                let range = instances.range();
                graph_budget::instances(slice(bytes, range.clone())?, range.start)?;
                for instance in instances {
                    self.instance(instance.map_err(|_| invalid())?)?;
                }
            }
            Payload::ComponentCanonicalSection(functions) if functions.count() == 1 => {
                let range = functions.range();
                graph_budget::canonical(slice(bytes, range.clone())?, range.start)?;
                for function in functions {
                    match function.map_err(|_| invalid())? {
                        CanonicalFunction::Lower { func_index: 0, options }
                            if self.phase == 4
                                && options.as_ref()
                                    == [
                                        CanonicalOption::UTF8,
                                        CanonicalOption::Memory(0),
                                        CanonicalOption::Realloc(0),
                                    ] =>
                        {
                            self.phase = 5;
                        }
                        CanonicalFunction::Lift { core_func_index: 2, options, .. }
                            if self.phase == 8 && options.is_empty() =>
                        {
                            self.phase = 9;
                        }
                        _ => return Err(invalid()),
                    }
                }
            }
            Payload::ComponentInstanceSection(instances)
                if self.phase == 9 && instances.count() == 1 =>
            {
                self.public_instance(bytes, instances)?;
            }
            Payload::ComponentExportSection(exports)
                if self.phase == 10 && exports.count() == 1 =>
            {
                self.public_export(exports)?;
            }
            _ => return Err(invalid()),
        }
        Ok(())
    }

    fn public_instance(
        &mut self,
        bytes: &[u8],
        instances: wasmparser::ComponentInstanceSectionReader<'_>,
    ) -> Result<(), Diagnostic> {
        let range = instances.range();
        graph_budget::component_instance(slice(bytes, range.clone())?, range.start)?;
        for instance in instances {
            let ComponentInstance::FromExports(exports) = instance.map_err(|_| invalid())? else {
                return Err(invalid());
            };
            let [export] = exports.as_ref() else { return Err(invalid()) };
            type_budget::external_name(export.name)?;
            if export.name.name != "run"
                || export.kind != ComponentExternalKind::Func
                || export.index != 1
                || export.ty.is_some()
            {
                return Err(invalid());
            }
        }
        self.phase = 10;
        Ok(())
    }

    fn public_export(
        &mut self,
        exports: wasmparser::ComponentExportSectionReader<'_>,
    ) -> Result<(), Diagnostic> {
        for export in exports {
            let export = export.map_err(|_| invalid())?;
            type_budget::external_name(export.name)?;
            let Some(ComponentTypeRef::Instance(index)) = export.ty else { return Err(invalid()) };
            if export.name.name != "wasi:cli/run@0.2.12"
                || export.kind != ComponentExternalKind::Instance
                || export.index != 16
            {
                return Err(invalid());
            }
            self.budget.interface_use(index, false)?;
        }
        self.phase = 11;
        Ok(())
    }

    fn alias(&mut self, alias: &ComponentAlias<'_>) -> Result<(), Diagnostic> {
        match alias {
            ComponentAlias::InstanceExport {
                kind: ComponentExternalKind::Func,
                instance_index,
                name: "get-environment",
            } if self.phase == 0
                && self.imports == 16
                && Some(*instance_index) == self.environment =>
            {
                self.phase = 1;
            }
            ComponentAlias::CoreInstanceExport {
                kind: ExternalKind::Memory,
                instance_index: 0,
                name: "memory",
            } if self.phase == 2 => self.phase = 3,
            ComponentAlias::CoreInstanceExport {
                kind: ExternalKind::Func,
                instance_index: 0,
                name: "realloc",
            } if self.phase == 3 => self.phase = 4,
            ComponentAlias::CoreInstanceExport {
                kind: ExternalKind::Func,
                instance_index: 2,
                name: "run",
            } if self.phase == 7 => self.phase = 8,
            ComponentAlias::InstanceExport { kind: ComponentExternalKind::Type, .. }
                if self.phase == 0 =>
            {
                self.budget.alias(alias)?;
            }
            _ => return Err(invalid()),
        }
        Ok(())
    }

    fn instance(&mut self, instance: Instance<'_>) -> Result<(), Diagnostic> {
        match instance {
            Instance::Instantiate { module_index: 0, args }
                if self.phase == 1 && args.is_empty() =>
            {
                self.phase = 2;
            }
            Instance::FromExports(exports) if self.phase == 5 && exports.len() == 1 => {
                let export = &exports[0];
                if export.name != "get-environment"
                    || export.kind != ExternalKind::Func
                    || export.index != 1
                {
                    return Err(invalid());
                }
                self.phase = 6;
            }
            Instance::Instantiate { module_index: 1, args }
                if self.phase == 6
                    && args.len() == 2
                    && args[0].name == "storage"
                    && args[0].index == 0
                    && args[1].name == "host"
                    && args[1].index == 1 =>
            {
                self.phase = 7;
            }
            _ => return Err(invalid()),
        }
        Ok(())
    }
}

fn slice(bytes: &[u8], range: Range<u64>) -> Result<&[u8], Diagnostic> {
    let start = usize::try_from(range.start).map_err(|_| invalid())?;
    let end = usize::try_from(range.end).map_err(|_| invalid())?;
    bytes.get(start..end).ok_or_else(invalid)
}

fn invalid() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4104",
        None,
        "Command component differs from its retained cores, canonical options or exact execution graph.",
        "Retain the audited storage and language cores and the unchanged authenticated command world.",
    )
}
