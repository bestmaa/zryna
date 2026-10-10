//! Borrow the unchanged authenticated full resolver, then select the server world explicitly.

use wasm_encoder::ValType;
use wit_parser::{
    Resolve, WorldId,
    abi::{AbiVariant, WasmType},
};
use zryna_diagnostics::Diagnostic;

use super::invalid;
use crate::{WitSource, WitWorldAudit, wit_world_audit::AuthenticatedCommandWorld};

pub(super) const ID: &str = "zryna:capability-profiles/server@0.1.0";
pub(super) const HTTP: &str = "wasi:http/types@0.2.12";
pub(super) const CLOCK: &str = "wasi:clocks/monotonic-clock@0.2.12";
pub(super) const HANDLER: &str = "wasi:http/incoming-handler@0.2.12";
pub(super) const OPERATIONS: [&str; 6] = [
    "[constructor]fields",
    "[constructor]outgoing-response",
    "[method]outgoing-response.set-status-code",
    "[method]outgoing-response.body",
    "[static]outgoing-body.finish",
    "[static]response-outparam.set",
];

pub(super) struct World {
    authority: AuthenticatedCommandWorld,
    server: WorldId,
}

impl World {
    pub(super) fn new(sources: &[WitSource]) -> Result<Self, Diagnostic> {
        // This supplier authenticates and audits all three worlds, not just command.
        // No command emission, grant or runtime interface is used or modified here.
        let authority = AuthenticatedCommandWorld::new(sources)?;
        let root = authority.resolve().worlds[authority.world()]
            .package
            .ok_or_else(|| invalid("authenticated root package is missing"))?;
        let server = authority
            .resolve()
            .select_world(&[root], Some(ID))
            .map_err(|_| invalid("authenticated server world cannot be selected"))?;
        Ok(Self { authority, server })
    }

    pub(super) fn resolve(&self) -> &Resolve {
        self.authority.resolve()
    }
    pub(super) const fn world(&self) -> WorldId {
        self.server
    }
    pub(super) fn audit(&self) -> &WitWorldAudit {
        self.authority.audit()
    }

    pub(super) fn signature(
        &self,
        interface: &str,
        operation: &str,
    ) -> Result<(Vec<ValType>, Vec<ValType>), Diagnostic> {
        let resolve = self.resolve();
        let (_, interface) = resolve
            .interfaces
            .iter()
            .find(|(id, _)| resolve.id_of(*id).as_deref() == Some(interface))
            .ok_or_else(|| invalid("server ABI interface is missing"))?;
        let function = interface
            .functions
            .get(operation)
            .ok_or_else(|| invalid("server ABI operation is missing"))?;
        let signature = resolve.wasm_signature(AbiVariant::GuestImport, function);
        if signature.indirect_params {
            return Err(invalid("server ABI unexpectedly requires indirect parameters"));
        }
        let convert = |ty| match ty {
            WasmType::I32 | WasmType::Pointer | WasmType::Length => ValType::I32,
            WasmType::I64 | WasmType::PointerOrI64 => ValType::I64,
            WasmType::F32 => ValType::F32,
            WasmType::F64 => ValType::F64,
        };
        Ok((
            signature.params.into_iter().map(convert).collect(),
            signature.results.into_iter().map(convert).collect(),
        ))
    }
}
