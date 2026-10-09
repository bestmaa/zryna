//! Closed execution observations. An editable manifest cannot reconstruct run authority.

mod model;
mod observation;
use observation::{component, execution, grant, hex, limits, registry_ceilings};
#[cfg(test)]
mod tests;
mod validation;
mod wire;

use super::{
    CommandH1ExecutionRecord, CommandH1Outcome, CommandH1RunReturn, CommandH1Teardown,
    CommandH1TrapCategory, ExecutedCommandH1, envelope,
};
use model::{
    Component, Composition, Denial, Document, Execution, Grant, Grants, Input, InputKind, Limits,
    OutcomeKind, RunReturn, ScalarEntry, Source, Teardown, TrapCategory,
};
use wire::Optional;
use zryna_diagnostics::Diagnostic;

/// Exact create-only command bundle manifest filename.
pub const COMMAND_H1_MANIFEST_NAME: &str = "zryna-wasi-command-manifest-v1.json";
/// Distinct command execution observation schema.
pub const COMMAND_H1_MANIFEST_SCHEMA: &str = "zryna.wasi-command-manifest.v1";
/// Hard serialized manifest limit, including optional JSON whitespace.
pub const MAX_COMMAND_H1_MANIFEST_BYTES: usize = 16 * 1024;
const PROFILE: &str = "command-h1-v1";
const WORLD: &str = "zryna:capability-profiles/command@0.1.0";
const ENVIRONMENT: &str = "wasi:cli/environment@0.2.12";
const POLICY: &str = "zryna.wit-capability-profiles.v1";
const HOST_POLICY: &str = "zryna.command-h1.host.v1";
const INTERFACE_FAILURE: &str = "zryna.command.interface-violation.v1";

fn invalid() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-D4103",
        None,
        "Command manifest observation is invalid.",
        "Use the closed command execution manifest and its retained compiler artifacts.",
    )
}

/// A structurally and semantically checked observation, never an execution or grant seal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandH1Manifest {
    document: Document,
    record: CommandH1ExecutionRecord,
}

impl CommandH1Manifest {
    /// Returns the recorded portable source path.
    #[must_use]
    pub fn source_path(&self) -> &str {
        &self.document.source.path
    }
    /// Returns the recorded component's relative bundle path.
    #[must_use]
    pub fn component_path(&self) -> &str {
        &self.document.component.path
    }
    /// Returns the claimed execution observation, without authenticating an actual run.
    #[must_use]
    pub const fn record(&self) -> &CommandH1ExecutionRecord {
        &self.record
    }
    /// Returns only the captured input's recorded presence category.
    #[must_use]
    pub const fn input_kind(&self) -> &'static str {
        match self.document.input.kind {
            InputKind::None => "none",
            InputKind::Missing => "missing",
            InputKind::Present => "present",
        }
    }
    /// Returns the bounded recorded UTF-8 count, never the input value or a value hash.
    #[must_use]
    pub const fn input_byte_count(&self) -> u32 {
        self.document.input.utf8_byte_count
    }
}

/// Decodes one bounded, closed manifest and rejects duplicate, unknown or inconsistent fields.
/// This validates observations only; it cannot mint a prepared or executed command.
///
/// # Errors
/// Rejects invalid UTF-8/JSON, trailing input, alternate shapes or inconsistent observations.
pub fn decode_command_h1_manifest(bytes: &[u8]) -> Result<CommandH1Manifest, Diagnostic> {
    if bytes.len() > MAX_COMMAND_H1_MANIFEST_BYTES {
        return Err(invalid());
    }
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let document: Document = wire::object(&mut decoder).map_err(|_| invalid())?;
    decoder.end().map_err(|_| invalid())?;
    let record = validation::validate(&document)?;
    Ok(CommandH1Manifest { document, record })
}

impl ExecutedCommandH1 {
    /// Derives a non-secret manifest only from this consumed run's retained authorities.
    /// The original private capture remains held by this object through publication.
    ///
    /// # Errors
    /// Rejects an invalid artifact stem or inconsistent retained execution observations.
    pub fn manifest_bytes(&self, stem: &str) -> Result<Vec<u8>, Diagnostic> {
        crate::javascript::validate_artifact_stem(stem).map_err(|_| invalid())?;
        let document = document(self, stem)?;
        validation::validate(&document)?;
        let bytes = serde_json::to_vec(&document).map_err(|_| invalid())?;
        if bytes.len() > MAX_COMMAND_H1_MANIFEST_BYTES {
            return Err(invalid());
        }
        Ok(bytes)
    }
}

fn document(run: &ExecutedCommandH1, stem: &str) -> Result<Document, Diagnostic> {
    let authority = &run.authority;
    let program = authority.compiled.verified_ir();
    let artifact = &authority.artifact;
    if !program.source().is_bound_to(&authority.sources) || program.identity() != artifact.issuer()
    {
        return Err(invalid());
    }
    let file = program
        .source()
        .syntax()
        .files()
        .first()
        .and_then(|file| authority.sources.source(file.id()))
        .ok_or_else(invalid)?;
    let entries = program
        .modules()
        .flat_map(zryna_ir::data_ownership_v1::VerifiedModule::functions)
        .filter_map(zryna_ir::data_ownership_v1::VerifiedFunction::public_export)
        .collect::<Vec<_>>();
    let [entry] = entries.as_slice() else { return Err(invalid()) };
    let required = grant(authority.key());
    let requested = grant(authority.request.as_ref().map(|request| request.request().key()));
    let approved = grant(authority.policy.key());
    // Post-run revocation or privacy changes must not erase the observed denial/trap.
    // These are immutable admission metadata, not a new pathname read or active-policy check.
    let input = match &authority.request {
        None => Input { kind: InputKind::None, utf8_byte_count: 0 },
        Some(request) => Input {
            kind: if request.request().value().is_some() {
                InputKind::Present
            } else {
                InputKind::Missing
            },
            utf8_byte_count: u32::try_from(request.request().value_byte_count())
                .map_err(|_| invalid())?,
        },
    };
    Ok(Document {
        schema: COMMAND_H1_MANIFEST_SCHEMA.into(),
        source: Source {
            path: file.path().as_str().into(),
            sha256: hex(artifact.source_digest()),
            profile: PROFILE.into(),
            verifier_revision: 1,
            program_binding: hex(artifact.program_binding()),
            requirements: required,
            scalar_entry: ScalarEntry {
                name: entry.logical_name().as_str().into(),
                abi_version: 1,
                abi_index: u32::try_from(entry.index()).map_err(|_| invalid())?,
                parameters: entry.parameters().to_vec(),
                result: entry.result(),
            },
        },
        composition: Composition {
            binding: hex(authority.composition.identity()),
            root: "command-source".into(),
            language: "CommandH1V1".into(),
            row: "WitCommand".into(),
            world: WORLD.into(),
            policy_version: POLICY.into(),
            approved: approved.clone(),
            static_quota: authority.composition.quota(),
        },
        component: component(authority, stem)?,
        grants: Grants {
            requested,
            effective: approved,
            registry_ceilings: registry_ceilings()?,
            static_quota: authority.composition.quota(),
            host_policy: HOST_POLICY.into(),
        },
        input,
        limits: limits(),
        execution: execution(run.record()),
        teardown: match run.record().teardown() {
            CommandH1Teardown::Confirmed => Teardown::Confirmed,
            CommandH1Teardown::Unconfirmed => Teardown::Unconfirmed,
        },
    })
}
