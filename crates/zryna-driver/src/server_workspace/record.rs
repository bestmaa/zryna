//! Bounded public result material contains identities and counters, never request bodies.

use super::{
    CommandFailure,
    approval::{Inputs, digest},
    failure,
};
use crate::{server_runtime, server_transport};
use serde_json::{Value, json};
use std::{net::SocketAddr, sync::atomic::Ordering};

pub(super) const NAME: &str = "zryna-wasi-server-manifest-v1.json";
pub(super) const CLOCK_NAME: &str = "zryna-wasi-server-manifest-v2.json";

pub(super) fn material(
    prepared: &server_runtime::Prepared,
    entrypoint: &str,
    source: &[u8],
    export: &str,
    stem: &str,
    inputs: &Inputs,
) -> Result<Value, CommandFailure> {
    let artifact = prepared.artifact();
    let world = artifact
        .world_audit()
        .worlds()
        .iter()
        .find(|world| world.identity() == "zryna:capability-profiles/server@0.1.0")
        .ok_or_else(|| failure("ZRYNA-C4203", "Authenticated server world is absent."))?;
    let wit = zryna_backend_webassembly::pinned_wit_sources()
        .iter()
        .map(|source| json!({"path":source.path(), "sha256":digest(source.bytes())}))
        .collect::<Vec<_>>();
    let config = inputs.config;
    let mut record = json!({
        "schema":"zryna.wasi-server-manifest.v1", "profile":"server-status-v1",
        "source":{"path":entrypoint,"sha256":digest(source)}, "export":export,
        "component":{"path":format!("component/{stem}.wasm"),"sha256":digest(artifact.bytes()),
            "bytes":artifact.bytes().len(),"bridge_revision":artifact.bridge_revision()},
        "world":{"identity":world.identity(),"imports":world.resolved_imports(),"exports":world.exports(),"wit":wit},
        "binding_sha256":digest(prepared.binding()), "requested_guest_grants":[],"effective_guest_grants":[],
        "listener_approval_identity":inputs.approval_identity()?,
        "configuration_sha256":inputs.configuration_digest(),
        "configuration":{"listen":config.address.to_string(),"attempts":config.attempts,
            "header_bytes":config.header_bytes,"body_bytes":config.body_bytes,
            "request_ms":config.request_timeout.as_millis(),"service_ms":config.service_timeout.as_millis()},
        "runtime_limits":{"memory_bytes":65_536,"fuel":100_000,"resources":5,"callbacks":8,"concurrency":1}
    });
    if let Some(guest) = &inputs.guest {
        guest.add_record(&mut record)?;
    }
    Ok(record)
}

pub(super) fn complete(
    mut material: Value,
    address: SocketAddr,
    result: Result<server_transport::Completion, server_transport::Error>,
    transport: &server_transport::Observation,
    runtime: &server_runtime::Observation,
) -> Result<Value, CommandFailure> {
    let count = |counter: &std::sync::atomic::AtomicUsize| counter.load(Ordering::SeqCst);
    let accepted = count(&transport.accepted);
    let served = count(&transport.served);
    let rejected = count(&transport.rejected);
    if count(&transport.listeners) != 0
        || count(&transport.socket_handles) != 0
        || count(&transport.reserved_bytes) != 0
        || runtime.live() != (0, 0)
        || count(&runtime.input_copies) != 0
        || count(&runtime.input_copy_bytes) != 0
        || count(&runtime.stores_created) != count(&runtime.stores_destroyed)
        || count(&runtime.created) != count(&runtime.destroyed)
    {
        return Err(failure("ZRYNA-C4204", "Actual server teardown was not confirmed."));
    }
    let outcome = match result {
        Ok(server_transport::Completion::Attempts) => "attempts_exhausted",
        Ok(server_transport::Completion::Deadline) => "service_deadline",
        Ok(server_transport::Completion::Cancelled) => "cancelled",
        Err(_) => "host_failure",
    };
    material["endpoint"] = json!(address.to_string());
    material["execution"] = json!({"outcome":outcome,"accepted":accepted,"served":served,"rejected":rejected,
        "teardown":{"confirmed":true,"stores_created":count(&runtime.stores_created),
            "stores_destroyed":count(&runtime.stores_destroyed),"resources_created":count(&runtime.created),
            "resources_destroyed":count(&runtime.destroyed),"listeners":0,"socket_handles":0,"reserved_bytes":0}});
    if material["profile"] == "server-clock-status-v1" {
        let reads = count(&runtime.clock_reads);
        if reads > count(&runtime.stores_created) {
            return Err(failure("ZRYNA-C4204", "Observed guest clock reads exceeded their bound."));
        }
        material["execution"]["clock_reads"] = json!(reads);
        material["execution"]["denied_callbacks"] = json!(count(&runtime.denials));
    }
    let bytes = serde_json::to_vec(&material)
        .map_err(|_| failure("ZRYNA-C4205", "Server record encoding failed."))?;
    if bytes.len() > 65_536 {
        return Err(failure("ZRYNA-C4205", "Server record exceeded its bound."));
    }
    Ok(material)
}

pub(super) fn stage(
    material: Value,
    address: SocketAddr,
    result: Result<server_transport::Completion, server_transport::Error>,
    transport: &server_transport::Observation,
    runtime: &server_runtime::Observation,
    transaction: &crate::pipeline::Transaction,
    name: &str,
) -> Result<Value, CommandFailure> {
    let manifest = complete(material, address, result, transport, runtime)?;
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|_| failure("ZRYNA-C4205", "Server result encoding failed."))?;
    if bytes.len() > 65_536 {
        return Err(failure("ZRYNA-C4205", "Server result exceeds its byte bound."));
    }
    transaction
        .write_manifest(name, &bytes)
        .map_err(|_| failure("ZRYNA-C4205", "Server manifest publication failed."))?;
    Ok(manifest)
}
