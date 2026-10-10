//! Bounded final-byte topology and binary validation, canonical replay, then decoded world audit.

use wasmparser::{Encoding, Parser, Payload, Validator, WasmFeatures};
use wit_parser::{Resolve, WorldId, WorldItem, decoding::DecodedWasm};
use zryna_diagnostics::Diagnostic;

use super::{
    ServerOperation, bridge, invalid, memory, shell,
    world::{ID, World},
};
use crate::ValidatedWebAssemblyArtifact;

pub(super) fn audit(
    bytes: &[u8],
    core: &ValidatedWebAssemblyArtifact,
    world: &World,
    export: &str,
    operation: ServerOperation,
) -> Result<(), Diagnostic> {
    if bytes.len() > shell::MAX_BYTES {
        return Err(invalid("server component exceeds 1048576 bytes"));
    }
    let modules = topology(bytes)?;
    if modules[0] != core.bytes()
        || modules[1] != memory::encode()
        || modules[2] != bridge::encode(world, export, operation)?
    {
        return Err(invalid("server component substituted a core, allocator or response bridge"));
    }
    // Reject any byte mutation before invoking recursive type validation/decoding. The replay
    // is derived from authenticated WIT and the retained compiler output, not a claimed digest.
    if bytes != shell::encode(world, core, export, operation)? {
        return Err(invalid("server component topology or canonical arrangement differs"));
    }
    for module in modules {
        Validator::new_with_features(WasmFeatures::WASM1)
            .validate_all(module)
            .map_err(|_| invalid("server retained core failed WebAssembly 1.0 validation"))?;
    }
    crate::audit_profile(core.bytes())?;
    Validator::new_with_features(WasmFeatures::WASM1.union(WasmFeatures::COMPONENT_MODEL))
        .validate_all(bytes)
        .map_err(|_| invalid("server component failed complete binary and type validation"))?;
    let DecodedWasm::Component(resolve, decoded) = wit_parser::decoding::decode(bytes)
        .map_err(|_| invalid("server component public world cannot be decoded"))?
    else {
        return Err(invalid("server bytes are a WIT package rather than an executable component"));
    };
    if resolve.types.len() > 4096 || resolve.interfaces.len() > 32 {
        return Err(invalid("decoded server world exceeds its type envelope"));
    }
    let expected = world
        .audit()
        .worlds()
        .iter()
        .find(|candidate| candidate.identity() == ID)
        .ok_or_else(|| invalid("authenticated server world observation is missing"))?;
    if interfaces(&resolve, decoded, false)? != expected.resolved_imports()
        || interfaces(&resolve, decoded, true)? != expected.exports()
    {
        return Err(invalid("decoded server interfaces or versions differ from authenticated WIT"));
    }
    Ok(())
}

fn interfaces(resolve: &Resolve, world: WorldId, export: bool) -> Result<Vec<String>, Diagnostic> {
    let world = &resolve.worlds[world];
    let items = if export { &world.exports } else { &world.imports };
    let mut names = Vec::new();
    for item in items.values() {
        let WorldItem::Interface { id, .. } = item else {
            return Err(invalid("decoded server world contains a non-interface item"));
        };
        names.push(
            resolve.id_of(*id).ok_or_else(|| invalid("decoded server interface is unnamed"))?,
        );
    }
    names.sort();
    Ok(names)
}

fn topology(bytes: &[u8]) -> Result<[&[u8]; 3], Diagnostic> {
    let mut modules = Vec::new();
    let mut in_module = false;
    let mut payloads = 0;
    let mut imports = 0;
    let mut exports = 0;
    for payload in Parser::new(0).parse_all(bytes) {
        payloads += 1;
        if payloads > 256 {
            return Err(invalid("server component exceeds 256 payloads"));
        }
        let payload = payload.map_err(|_| invalid("server component payload is malformed"))?;
        if in_module {
            match payload {
                Payload::End(_) => in_module = false,
                Payload::Version { encoding: Encoding::Module, .. }
                | Payload::TypeSection(_)
                | Payload::ImportSection(_)
                | Payload::FunctionSection(_)
                | Payload::MemorySection(_)
                | Payload::GlobalSection(_)
                | Payload::ExportSection(_)
                | Payload::CodeSectionStart { .. }
                | Payload::CodeSectionEntry(_) => {}
                _ => {
                    return Err(invalid(
                        "server core contains a start, nesting or unsupported section",
                    ));
                }
            }
            continue;
        }
        match payload {
            Payload::Version { encoding: Encoding::Component, .. } | Payload::End(_) => {}
            Payload::ModuleSection { unchecked_range, .. } => {
                if modules.len() == 3 {
                    return Err(invalid("server component has extra core modules"));
                }
                let start = usize::try_from(unchecked_range.start)
                    .map_err(|_| invalid("server core offset exceeds the host address range"))?;
                let end = usize::try_from(unchecked_range.end)
                    .map_err(|_| invalid("server core offset exceeds the host address range"))?;
                modules.push(
                    bytes
                        .get(start..end)
                        .ok_or_else(|| invalid("server core range is outside component bytes"))?,
                );
                in_module = true;
            }
            Payload::ComponentTypeSection(types) if types.count() <= 4096 => {}
            Payload::ComponentImportSection(items) => {
                imports += items.count();
                if imports > 8 {
                    return Err(invalid("server component has extra imports"));
                }
            }
            Payload::ComponentAliasSection(items) if items.count() <= 128 => {}
            Payload::InstanceSection(items) if items.count() <= 4 => {}
            Payload::ComponentInstanceSection(items) if items.count() == 1 => {}
            Payload::ComponentCanonicalSection(items) if items.count() <= 9 => {}
            Payload::ComponentExportSection(items) => {
                exports += items.count();
                if exports > 1 {
                    return Err(invalid("server component has extra exports"));
                }
            }
            _ => {
                return Err(invalid(
                    "server component has a start, nesting or unsupported section",
                ));
            }
        }
    }
    if in_module || imports != 8 || exports != 1 {
        return Err(invalid("server component is missing its exact world arrangement"));
    }
    modules.try_into().map_err(|_| invalid("server component requires exactly three core modules"))
}
