//! Deterministic Component Model wrapping for the verified scalar profile.

use std::borrow::Cow;

use sha2::{Digest, Sha256};
use wasm_encoder::{
    ComponentBuilder, ComponentExportKind, ComponentTypeRef, ComponentValType, CustomSection,
    ExportKind, PrimitiveValType,
};
use zryna_diagnostics::Diagnostic;
use zryna_ir::{Type, VerifiedProgram};

use crate::{
    ValidatedWebAssemblyArtifact, WitSource, WitWorldAudit,
    wit_world_audit::{AuthenticatedBrowserWorld, BROWSER_WORLD},
};

mod audit;

#[cfg(test)]
mod tests;

const METADATA_SECTION: &str = "zryna-component-v1";
const METADATA_REVISION: &str = "zryna.scalar-component.v1";

/// One authenticated scalar export retained by the audited component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScalarExport {
    logical: String,
    component: String,
    webassembly: String,
    arity: usize,
}

impl ScalarExport {
    /// Exact source-level export name.
    #[must_use]
    pub fn logical_name(&self) -> &str {
        &self.logical
    }
    /// Collision-free Component Model export label.
    #[must_use]
    pub fn component_name(&self) -> &str {
        &self.component
    }
    /// Exact retained core-module export name.
    #[must_use]
    pub fn webassembly_name(&self) -> &str {
        &self.webassembly
    }
    /// Number of verified `i32` parameters.
    #[must_use]
    pub const fn arity(&self) -> usize {
        self.arity
    }
}

/// One deterministic Component Model artifact sealed to verified scalar IR and pinned WIT.
pub struct ValidatedScalarComponent {
    bytes: Vec<u8>,
    core: ValidatedWebAssemblyArtifact,
    world: AuthenticatedBrowserWorld,
    digest: [u8; 32],
    core_digest: [u8; 32],
    interface_digest: [u8; 32],
    exports: Vec<ScalarExport>,
}

impl ValidatedScalarComponent {
    /// Returns the exact validated Component Model bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the unchanged compiler-produced core module retained by the component.
    #[must_use]
    pub const fn core(&self) -> &ValidatedWebAssemblyArtifact {
        &self.core
    }

    /// Returns the exact capability-world identity bound to this artifact.
    #[must_use]
    pub const fn world_identity(&self) -> &'static str {
        BROWSER_WORLD
    }

    /// Returns the authenticated pinned WIT dependency observation.
    #[must_use]
    pub fn world_audit(&self) -> &WitWorldAudit {
        self.world.audit()
    }

    /// Returns the SHA-256 identity of the complete component.
    #[must_use]
    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    /// Returns the SHA-256 identity of the retained core module.
    #[must_use]
    pub const fn core_digest(&self) -> &[u8; 32] {
        &self.core_digest
    }

    /// Returns the SHA-256 identity of the ordered public scalar interface.
    #[must_use]
    pub const fn interface_digest(&self) -> &[u8; 32] {
        &self.interface_digest
    }

    /// Returns only the sealed, ordered scalar interface derived from verified IR.
    #[must_use]
    pub fn exports(&self) -> &[ScalarExport] {
        &self.exports
    }

    /// SHA-256 of the complete authenticated WIT source closure.
    #[must_use]
    pub const fn wit_source_digest(&self) -> &[u8; 32] {
        self.world.source_digest()
    }

    /// Revalidates the sealed artifact against matching verified IR and WIT sources.
    ///
    /// # Errors
    ///
    /// Rejects any changed program, interface, core, component or WIT dependency closure.
    pub fn revalidate(
        &self,
        program: &VerifiedProgram,
        sources: &[WitSource],
    ) -> Result<(), Diagnostic> {
        let rebuilt = emit_scalar_component(program, sources)?;
        if rebuilt.bytes != self.bytes
            || rebuilt.digest != self.digest
            || rebuilt.core_digest != self.core_digest
            || rebuilt.interface_digest != self.interface_digest
        {
            return Err(invalid(
                "scalar component no longer matches its verified program and WIT identity",
            ));
        }
        Ok(())
    }
}

/// Emits one audited, capability-free component from the default verified `I32V1` profile.
///
/// The component retains the unchanged core module and exposes each verified export through an
/// exact canonical lift. It does not instantiate a host, generate bindings or grant capabilities.
///
/// # Errors
///
/// Rejects substituted WIT inputs, unsupported scalar signatures, resource excess, malformed
/// final topology, or any mismatch between the retained core and component interface.
pub fn emit_scalar_component(
    program: &VerifiedProgram,
    sources: &[WitSource],
) -> Result<ValidatedScalarComponent, Diagnostic> {
    let world = AuthenticatedBrowserWorld::new(sources)?;
    let exports = exports(program)?;
    let core = crate::emit(program)?;
    let metadata = metadata(world.source_digest());
    let bytes = encode(&core, &exports, &metadata);
    audit::audit(&bytes, &core, &exports, &metadata, &world)?;
    let interface_digest = interface_digest(&exports);
    Ok(ValidatedScalarComponent {
        digest: digest(&bytes),
        core_digest: digest(core.bytes()),
        bytes,
        core,
        world,
        interface_digest,
        exports,
    })
}

fn exports(program: &VerifiedProgram) -> Result<Vec<ScalarExport>, Diagnostic> {
    program
        .functions()
        .map(|function| {
            if function.parameters().iter().any(|ty| *ty != Type::I32)
                || function.return_type() != Type::I32
            {
                return Err(invalid("component export is outside the verified i32 profile"));
            }
            let logical = function.export_name().as_str().to_owned();
            Ok(ScalarExport {
                component: component_export_name(&logical),
                logical,
                webassembly: function.abi_export().webassembly_name().as_str().to_owned(),
                arity: function.parameters().len(),
            })
        })
        .collect()
}

fn encode(
    core: &ValidatedWebAssemblyArtifact,
    exports: &[ScalarExport],
    metadata: &str,
) -> Vec<u8> {
    let mut component = ComponentBuilder::default();
    component.custom_section(&CustomSection {
        name: Cow::Borrowed(METADATA_SECTION),
        data: Cow::Borrowed(metadata.as_bytes()),
    });
    let module = component.core_module_raw(None, core.bytes());
    let instance = component.core_instantiate(None, module, []);
    let core_functions = exports
        .iter()
        .map(|export| {
            component.core_alias_export(
                Some(&export.logical),
                instance,
                &export.webassembly,
                ExportKind::Func,
            )
        })
        .collect::<Vec<_>>();
    let function_types = exports
        .iter()
        .map(|export| {
            let names = (0..export.arity).map(|index| format!("arg{index}")).collect::<Vec<_>>();
            let (index, mut function) = component.type_function(Some(&export.logical));
            function.params(
                names.iter().map(|name| {
                    (name.as_str(), ComponentValType::Primitive(PrimitiveValType::S32))
                }),
            );
            function.result(Some(ComponentValType::Primitive(PrimitiveValType::S32)));
            index
        })
        .collect::<Vec<_>>();
    let functions = core_functions
        .into_iter()
        .zip(&function_types)
        .zip(exports)
        .map(|((core_function, function_type), export)| {
            component.lift_func(Some(&export.logical), core_function, *function_type, [])
        })
        .collect::<Vec<_>>();
    for ((function, function_type), export) in
        functions.into_iter().zip(function_types).zip(exports)
    {
        component.export(
            export.component.as_str(),
            ComponentExportKind::Func,
            function,
            Some(ComponentTypeRef::Func(function_type)),
        );
    }
    component.finish()
}

fn metadata(source_digest: &[u8; 32]) -> String {
    format!("{METADATA_REVISION}\nworld={BROWSER_WORLD}\nwit-sha256={}\n", hex(source_digest))
}

fn interface_digest(exports: &[ScalarExport]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"ZRYNA-SCALAR-COMPONENT-INTERFACE\0\x01");
    for export in exports {
        digest.update(u64::try_from(export.logical.len()).unwrap_or(u64::MAX).to_le_bytes());
        digest.update(export.logical.as_bytes());
        digest.update(u64::try_from(export.component.len()).unwrap_or(u64::MAX).to_le_bytes());
        digest.update(export.component.as_bytes());
        digest.update(u64::try_from(export.webassembly.len()).unwrap_or(u64::MAX).to_le_bytes());
        digest.update(export.webassembly.as_bytes());
        digest.update(u64::try_from(export.arity).unwrap_or(u64::MAX).to_le_bytes());
    }
    digest.finalize().into()
}

fn component_export_name(logical: &str) -> String {
    format!("zryna-export-{}", hex(logical.as_bytes()))
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn invalid(message: &str) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4020",
        None,
        message,
        "rebuild the component from matching verified scalar IR and pinned WIT sources",
    )
}
