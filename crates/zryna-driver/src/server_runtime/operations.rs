//! Minimal HTTP response resources plus explicitly approved monotonic reads.

use wasmtime::{
    AsContextMut, StoreContextMut,
    component::{ResourceDynamic, Val},
};

use super::{Error, host::Host, resources::Kind};

const HTTP: &str = "wasi:http/types@0.2.12";

pub(super) fn own(
    store: &mut impl AsContextMut<Data = Host>,
    name: &str,
    kind: Kind,
) -> Result<Val, Error> {
    let mut context = store.as_context_mut();
    let ty = context.data().ty(&format!("{HTTP}/{name}"))?;
    let rep = context.data_mut().resources.insert(kind, ty)?;
    let resource = ResourceDynamic::new_own(rep, ty)
        .try_into_resource_any(&mut context)
        .map_err(|_| Error::Resource)?;
    Ok(Val::Resource(resource))
}

fn handle(
    store: &mut StoreContextMut<'_, Host>,
    val: &Val,
    name: &str,
    owned: bool,
) -> Result<(u32, u32), Error> {
    let Val::Resource(resource) = val else {
        return Err(Error::Resource);
    };
    let resource = ResourceDynamic::try_from_resource_any(*resource, &mut *store)
        .map_err(|_| Error::Resource)?;
    let ty = store.data().ty(&format!("{HTTP}/{name}"))?;
    if resource.ty() != ty || resource.owned() != owned {
        return Err(Error::Resource);
    }
    // Dynamic lifting releases the transient Wasmtime host handle. The bounded resource registry
    // retains the actual object until this operation consumes it or the Store is destroyed.
    store.data_mut().resources.get_mut(resource.rep(), ty)?;
    Ok((resource.rep(), ty))
}

fn ok() -> Val {
    Val::Result(Ok(None))
}

pub(super) fn invoke(
    store: &mut StoreContextMut<'_, Host>,
    interface: &str,
    operation: &str,
    params: &[Val],
    results: &mut [Val],
) -> Result<(), Error> {
    store.data_mut().enter()?;
    if interface == "wasi:clocks/monotonic-clock@0.2.12" && operation == "now" {
        if !params.is_empty() || results.len() != 1 {
            return Err(Error::Artifact);
        }
        results[0] = Val::U64(store.data_mut().clock()?);
        return Ok(());
    }
    if interface != HTTP {
        return Err(store.data_mut().deny());
    }
    match (operation, params, results) {
        ("[constructor]fields", [], [result]) => {
            *result = own(store, "fields", Kind::Fields)?;
        }
        ("[constructor]outgoing-response", [fields], [result]) => {
            let (id, ty) = handle(store, fields, "fields", true)?;
            let fields = store.data_mut().resources.take(id, ty)?;
            if !matches!(fields.kind, Kind::Fields) {
                return Err(Error::Resource);
            }
            drop(fields);
            *result = own(
                store,
                "outgoing-response",
                Kind::Response { status: 200, body_started: false, finished: false },
            )?;
        }
        ("[method]outgoing-response.set-status-code", [response, Val::U16(status)], [result]) => {
            let (id, ty) = handle(store, response, "outgoing-response", false)?;
            let Kind::Response { status: current, finished, .. } =
                store.data_mut().resources.get_mut(id, ty)?
            else {
                return Err(Error::Resource);
            };
            if *finished || !(200..=599).contains(status) {
                return Err(Error::Guest);
            }
            *current = *status;
            *result = ok();
        }
        ("[method]outgoing-response.body", [response], [result]) => {
            let (id, ty) = handle(store, response, "outgoing-response", false)?;
            let Kind::Response { body_started, finished, .. } =
                store.data_mut().resources.get_mut(id, ty)?
            else {
                return Err(Error::Resource);
            };
            if *body_started || *finished {
                return Err(Error::Resource);
            }
            *body_started = true;
            *result = Val::Result(Ok(Some(Box::new(own(
                store,
                "outgoing-body",
                Kind::Body { response: id },
            )?))));
        }
        ("[static]outgoing-body.finish", [body, Val::Option(None)], [result]) => {
            let (id, ty) = handle(store, body, "outgoing-body", true)?;
            let body = store.data_mut().resources.take(id, ty)?;
            let Kind::Body { response } = body.kind else {
                return Err(Error::Resource);
            };
            let response_ty = store.data().ty(&format!("{HTTP}/outgoing-response"))?;
            let Kind::Response { body_started: true, finished, .. } =
                store.data_mut().resources.get_mut(response, response_ty)?
            else {
                return Err(Error::Resource);
            };
            if *finished {
                return Err(Error::Resource);
            }
            *finished = true;
            *result = ok();
        }
        ("[static]response-outparam.set", [outparam, Val::Result(Ok(Some(response)))], []) => {
            let (out_id, out_ty) = handle(store, outparam, "response-outparam", true)?;
            let (response_id, response_ty) = handle(store, response, "outgoing-response", true)?;
            let out = store.data_mut().resources.take(out_id, out_ty)?;
            let response = store.data_mut().resources.take(response_id, response_ty)?;
            if !matches!(out.kind, Kind::Outparam) {
                return Err(Error::Resource);
            }
            let Kind::Response { status, body_started: true, finished: true } = response.kind
            else {
                return Err(Error::Resource);
            };
            if store.data().response.is_some() {
                return Err(Error::Resource);
            }
            store.data_mut().response = Some(status);
        }
        _ => return Err(store.data_mut().deny()),
    }
    Ok(())
}
