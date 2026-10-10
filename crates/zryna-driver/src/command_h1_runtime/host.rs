//! Exact pinned imports mediate only approved captured environment input.

use super::Authority;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use wasmtime::{
    Engine, StoreLimits,
    component::{Component, Linker, ResourceType, Val, types::ComponentItem},
};

const ENVIRONMENT: &str = "wasi:cli/environment@0.2.12";

pub(super) struct Denial {
    pub(super) interface: String,
    pub(super) operation: String,
}

pub(super) struct Host {
    owner: Arc<()>,
    pub(super) authority: Arc<Authority>,
    pub(super) limits: StoreLimits,
    pub(super) denial: Option<Denial>,
    fatal: bool,
}

impl Host {
    pub(super) fn new(authority: Arc<Authority>) -> Self {
        Self {
            owner: Arc::new(()),
            authority,
            limits: super::envelope::limits(),
            denial: None,
            fatal: false,
        }
    }

    fn check_owner(&mut self, owner: &Arc<()>) -> wasmtime::Result<()> {
        if !Arc::ptr_eq(&self.owner, owner) {
            self.fatal = true;
            return Err(wasmtime::format_err!("command host store authority changed"));
        }
        Ok(())
    }

    fn deny(&mut self, interface: &str, operation: &str) -> wasmtime::Error {
        if self.denial.is_none() {
            self.denial = Some(Denial { interface: interface.into(), operation: operation.into() });
        }
        self.fatal = true;
        wasmtime::format_err!("command host permission denied")
    }

    pub(super) fn call(
        &mut self,
        interface: &str,
        operation: &str,
        parameters: &[Val],
        results: &mut [Val],
    ) -> wasmtime::Result<()> {
        if self.fatal {
            return Err(wasmtime::format_err!("command host store consumed"));
        }
        if interface != ENVIRONMENT {
            return Err(self.deny(interface, operation));
        }
        if !parameters.is_empty() || results.len() != 1 {
            self.fatal = true;
            return Err(wasmtime::format_err!("audited command host function type changed"));
        }
        match operation {
            "get-environment" => {
                let Ok(input) = self.authority.callback_input() else {
                    return Err(self.deny(interface, operation));
                };
                let values = input.value().map_or_else(Vec::new, |value| {
                    vec![Val::Tuple(vec![
                        Val::String(input.key().into()),
                        Val::String(value.into()),
                    ])]
                });
                if !self.authority.grant_matches() {
                    return Err(self.deny(interface, operation));
                }
                results[0] = Val::List(values);
                Ok(())
            }
            "get-arguments" => {
                results[0] = Val::List(Vec::new());
                Ok(())
            }
            "initial-cwd" => {
                results[0] = Val::Option(None);
                Ok(())
            }
            _ => Err(self.deny(interface, operation)),
        }
    }
}

pub(super) fn linker(
    engine: &Engine,
    component: &Component,
    host: &Host,
) -> wasmtime::Result<Linker<Host>> {
    static NEXT_RESOURCE: AtomicU32 = AtomicU32::new(65_536);
    let mut linker: Linker<Host> = Linker::new(engine);
    let mut resources = Vec::new();
    for (name, import) in component.component_type().imports(engine) {
        let ComponentItem::ComponentInstance(interface) = import.ty else {
            return Err(wasmtime::format_err!("audited command interface changed"));
        };
        let mut instance = linker.instance(name)?;
        for (operation, export) in interface.exports(engine) {
            match export.ty {
                ComponentItem::ComponentFunc(_) => {
                    let interface = name.to_owned();
                    let function_name = operation.to_owned();
                    let operation = function_name.clone();
                    let owner = Arc::clone(&host.owner);
                    instance.func_new(
                        &function_name,
                        move |mut store, _, parameters, results| {
                            store.data_mut().check_owner(&owner)?;
                            store.data_mut().call(&interface, &operation, parameters, results)
                        },
                    )?;
                }
                ComponentItem::Resource(guest_type) => {
                    let host_type = if let Some((_, host_type)) =
                        resources.iter().find(|(ty, _)| *ty == guest_type)
                    {
                        *host_type
                    } else {
                        let id = NEXT_RESOURCE
                            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| {
                                id.checked_add(1)
                            })
                            .map_err(|_| {
                                wasmtime::format_err!("command host resource identity exhausted")
                            })?;
                        let host_type = ResourceType::host_dynamic(id);
                        resources.push((guest_type, host_type));
                        host_type
                    };
                    let interface = name.to_owned();
                    let name = operation.to_owned();
                    let owner = Arc::clone(&host.owner);
                    instance.resource(operation, host_type, move |mut store, _| {
                        store.data_mut().check_owner(&owner)?;
                        Err(store.data_mut().deny(&interface, &name))
                    })?;
                }
                ComponentItem::Type(_) => {}
                _ => return Err(wasmtime::format_err!("audited command interface item changed")),
            }
        }
    }
    Ok(linker)
}
