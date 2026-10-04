//! Exact pinned interfaces, preserving resource aliases with fresh host identities.

use std::sync::atomic::{AtomicU32, Ordering};

use wasmtime::{
    Engine,
    component::{Component, Linker, ResourceType, types::ComponentItem},
};

use super::{Error, host::Host, operations};

static NEXT_TYPE: AtomicU32 = AtomicU32::new(1);
const IMPORTS: [&str; 8] = [
    "wasi:clocks/monotonic-clock@0.2.12",
    "wasi:clocks/wall-clock@0.2.12",
    "wasi:http/outgoing-handler@0.2.12",
    "wasi:http/types@0.2.12",
    "wasi:io/error@0.2.12",
    "wasi:io/poll@0.2.12",
    "wasi:io/streams@0.2.12",
    "wasi:random/random@0.2.12",
];

pub(super) fn bind(
    engine: &Engine,
    component: &Component,
    state: &mut Host,
) -> wasmtime::Result<Linker<Host>> {
    let mut linker: Linker<Host> = Linker::new(engine);
    let mut aliases = Vec::<(ResourceType, u32)>::new();
    let component_type = component.component_type();
    let imports = component_type.imports(engine);
    if imports.len() != IMPORTS.len() {
        return Err(Error::Artifact.into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut count = 0;
    for (name, import) in imports {
        if !IMPORTS.contains(&name)
            || !seen.insert(name.to_owned())
            || import.implements.is_some()
            || import.external_id.is_some()
        {
            return Err(Error::Artifact.into());
        }
        let ComponentItem::ComponentInstance(interface) = import.ty else {
            return Err(Error::Artifact.into());
        };
        let mut instance = linker.instance(name)?;
        for (operation, export) in interface.exports(engine) {
            count += 1;
            if count > 512
                || operation.len() > 256
                || export.implements.is_some()
                || export.external_id.is_some()
            {
                return Err(Error::Artifact.into());
            }
            match export.ty {
                ComponentItem::ComponentFunc(_) => {
                    let interface = name.to_owned();
                    let operation_name = operation.to_owned();
                    instance.func_new(operation, move |mut store, _, params, results| {
                        let result = operations::invoke(
                            &mut store,
                            &interface,
                            &operation_name,
                            params,
                            results,
                        );
                        if let Err(error) = &result {
                            store.data_mut().fatal = Some(*error);
                        }
                        result.map_err(Into::into)
                    })?;
                }
                ComponentItem::Resource(guest_type) => {
                    let ty = if let Some((_, ty)) = aliases.iter().find(|(t, _)| *t == guest_type) {
                        *ty
                    } else {
                        let ty = NEXT_TYPE
                            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| {
                                id.checked_add(1)
                            })
                            .map_err(|_| Error::Limit)?;
                        aliases.push((guest_type, ty));
                        ty
                    };
                    state.types.insert(format!("{name}/{operation}"), ty);
                    // Destruction releases request-owned resources even under empty grants.
                    // It cannot create authority or call an external provider.
                    instance.resource(
                        operation,
                        ResourceType::host_dynamic(ty),
                        move |mut store, rep| {
                            store.data_mut().resources.take(rep, ty)?;
                            Ok(())
                        },
                    )?;
                }
                ComponentItem::Type(_) => {}
                _ => return Err(Error::Artifact.into()),
            }
        }
    }
    Ok(linker)
}
