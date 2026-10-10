//! Independently specified pinned WASI signatures, rather than modified sealed artifacts.

mod core;
mod interfaces;

use wasm_encoder::{
    CanonicalOption, ComponentBuilder, ComponentExportKind, ComponentValType, ExportKind, ModuleArg,
};

#[derive(Clone, Copy)]
pub(super) enum Probe {
    Environment,
    Filesystem,
    Clock,
    Random,
    Network,
    Process,
}

impl Probe {
    pub(super) const ALL: [Self; 6] = [
        Self::Environment,
        Self::Filesystem,
        Self::Clock,
        Self::Random,
        Self::Network,
        Self::Process,
    ];

    pub(super) fn interface(self) -> &'static str {
        match self {
            Self::Environment => "wasi:cli/environment@0.2.12",
            Self::Filesystem => "wasi:filesystem/preopens@0.2.12",
            Self::Clock => "wasi:clocks/monotonic-clock@0.2.12",
            Self::Random => "wasi:random/random@0.2.12",
            Self::Network => "wasi:sockets/instance-network@0.2.12",
            Self::Process => "wasi:cli/exit@0.2.12",
        }
    }

    pub(super) fn operation(self) -> &'static str {
        match self {
            Self::Environment => "get-environment",
            Self::Filesystem => "get-directories",
            Self::Clock => "now",
            Self::Random => "get-random-u64",
            Self::Network => "instance-network",
            Self::Process => "exit-with-code",
        }
    }
}

pub(super) fn bytes(probe: Probe) -> Vec<u8> {
    let mut component = ComponentBuilder::default();
    let host = interfaces::import(&mut component, probe);
    let memory = component.core_module(None, &core::memory());
    let caller = component.core_module(None, &core::caller(probe));
    let owned = component.core_instantiate(None, memory, []);
    let memory = component.core_alias_export(None, owned, "memory", ExportKind::Memory);
    let realloc = component.core_alias_export(None, owned, "realloc", ExportKind::Func);
    let lowered = component.lower_func(
        None,
        host,
        [CanonicalOption::UTF8, CanonicalOption::Memory(memory), CanonicalOption::Realloc(realloc)],
    );
    let binding = component.core_instantiate_exports(None, [("deny", ExportKind::Func, lowered)]);
    let caller = component.core_instantiate(None, caller, [("host", ModuleArg::Instance(binding))]);
    let run = component.core_alias_export(None, caller, "run", ExportKind::Func);
    let (result_type, result) = component.type_defined(None);
    result.result(None, None);
    let (function_type, mut function) = component.type_function(None);
    function.params([] as [(&str, ComponentValType); 0]);
    function.result(Some(ComponentValType::Type(result_type)));
    let lifted = component.lift_func(None, run, function_type, []);
    component.export("run", ComponentExportKind::Func, lifted, None);
    component.finish()
}
