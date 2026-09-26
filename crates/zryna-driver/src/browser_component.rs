//! Deterministic browser bindings from one audited scalar component.

use std::fmt::Write;

use serde::Serialize;
use zryna_backend_webassembly::ValidatedScalarComponent;
use zryna_diagnostics::Diagnostic;

pub(crate) const REVISION: &str = "zryna.browser-component-bindings.v1";

pub(crate) struct Bindings {
    pub(crate) loader: Vec<u8>,
    pub(crate) declarations: Vec<u8>,
    pub(crate) core_offset: usize,
}

#[derive(Serialize)]
struct Export<'a> {
    logical: &'a str,
    component: &'a str,
    core: &'a str,
    arity: usize,
}

pub(crate) fn generate(component: &ValidatedScalarComponent) -> Result<Bindings, Diagnostic> {
    let core = component.core().bytes();
    if core.len() < 8 {
        return Err(invalid());
    }
    let mut positions =
        component.bytes().windows(8).enumerate().filter_map(|(index, candidate)| {
            (candidate == &core[..8]
                && component.bytes().get(index..index + core.len()) == Some(core))
            .then_some(index)
        });
    let Some(core_offset) = positions.next() else {
        return Err(invalid());
    };
    if positions.next().is_some() {
        return Err(invalid());
    }
    let exports = component
        .exports()
        .iter()
        .map(|export| Export {
            logical: export.logical_name(),
            component: export.component_name(),
            core: export.webassembly_name(),
            arity: export.arity(),
        })
        .collect::<Vec<_>>();
    let serialized = serde_json::to_string(&exports).map_err(|_| invalid())?;
    let loader = include_str!("browser_component/loader.js")
        .replace("__REVISION__", REVISION)
        .replace("__COMPONENT_SHA256__", &hex(component.digest()))
        .replace("__INTERFACE_SHA256__", &hex(component.interface_digest()))
        .replace("__WORLD__", component.world_identity())
        .replace("__COMPONENT_BYTES__", &component.bytes().len().to_string())
        .replace("__CORE_OFFSET__", &core_offset.to_string())
        .replace("__CORE_BYTES__", &core.len().to_string())
        .replace("__EXPORTS__", &serialized);
    let mut declarations = String::from("// Generated from an audited scalar component.\n");
    declarations.push_str("export interface BrowserComponent {\n");
    for export in component.exports() {
        let name = serde_json::to_string(export.logical_name()).map_err(|_| invalid())?;
        let arguments = (0..export.arity())
            .map(|index| format!("arg{index}: number"))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(&mut declarations, "  readonly [{name}]: ({arguments}) => number;")
            .map_err(|_| invalid())?;
    }
    declarations.push_str("}\n");
    declarations.push_str(
        "export declare const browserComponentIdentity: Readonly<{ revision: string; world: string; componentSha256: string; interfaceSha256: string }>;\n"
    );
    declarations.push_str(
        "export declare function instantiateBrowserComponent(bytes: Uint8Array, options?: { deadlineMs?: number }): Promise<BrowserComponent>;\n"
    );
    Ok(Bindings {
        loader: loader.into_bytes(),
        declarations: declarations.into_bytes(),
        core_offset,
    })
}

pub(crate) fn hex(bytes: &[u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn invalid() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-C3991",
        None,
        "audited component does not contain one exact retained core module",
        "rebuild from verified scalar source and the pinned browser world",
    )
}
