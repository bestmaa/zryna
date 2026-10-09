//! Complete source lowering, root approval and private captured input for one command run.

mod envelope;
mod host;
mod manifest;
mod policy;
mod preparation;
mod record;
mod session;
#[cfg(test)]
mod tests;

pub use manifest::{
    COMMAND_H1_MANIFEST_NAME, COMMAND_H1_MANIFEST_SCHEMA, CommandH1Manifest,
    MAX_COMMAND_H1_MANIFEST_BYTES, decode_command_h1_manifest,
};
pub use policy::CommandH1HostPolicy;
pub(crate) use preparation::prepare_approved_request;
pub use preparation::prepare_command_h1;
pub use record::{
    CommandH1ExecutionRecord, CommandH1Outcome, CommandH1RunReturn, CommandH1Teardown,
    CommandH1TrapCategory,
};

use crate::{command_request::CapturedRequest, profile_composition::CommandH1Composition};
use std::sync::Arc;
use zryna_backend_webassembly::{ValidatedCommandH1Artifact, WitSource};
use zryna_diagnostics::Diagnostic;
use zryna_source::SourceMap;

struct Authority {
    sources: SourceMap,
    compiled: zryna_semantics::command_h1_v1::VerifiedProgram,
    artifact: ValidatedCommandH1Artifact,
    wit: Vec<WitSource>,
    composition: CommandH1Composition,
    policy: CommandH1HostPolicy,
    request: Option<CapturedRequest>,
    diagnostics: Vec<Diagnostic>,
}

impl Authority {
    fn key(&self) -> Option<&str> {
        self.compiled
            .verified_ir()
            .source()
            .environment()
            .map(zryna_syntax::command_h1_v1::EnvironmentRequirement::key)
    }

    fn grant_matches(&self) -> bool {
        self.policy.active()
            && self.policy.key() == self.key()
            && self.request.as_ref().map(|captured| captured.request().key()) == self.key()
            && self.artifact.issuer() == self.compiled.verified_ir().identity()
            && self.artifact.program().source().is_bound_to(&self.sources)
            && self.composition.required_key() == self.key()
    }

    fn revalidate(&self) -> Result<(), Vec<Diagnostic>> {
        if !self.grant_matches() {
            return Err(vec![invalid()]);
        }
        self.artifact
            .revalidate(
                self.compiled.verified_ir(),
                self.compiled.runtime_abi(),
                &self.sources,
                &self.wit,
            )
            .map_err(|error| vec![error])?;
        self.composition.revalidate(
            &self.compiled,
            &self.artifact,
            &self.sources,
            self.policy.key(),
        )?;
        if let Some(request) = &self.request {
            request.revalidate().map_err(|error| vec![error])?;
        }
        Ok(())
    }

    fn callback_input(&self) -> Result<&crate::command_request::CommandRequest, Diagnostic> {
        if !self.grant_matches() {
            return Err(invalid());
        }
        let request = self.request.as_ref().ok_or_else(invalid)?;
        request.revalidate()?;
        Ok(request.request())
    }
}

/// A complete prepared command, constructed only by real authenticated source lowering.
/// It has one consuming execution method and exposes no input value or raw constructor.
pub struct PreparedCommandH1 {
    authority: Arc<Authority>,
}

impl PreparedCommandH1 {
    /// Returns the complete independently audited component artifact.
    #[must_use]
    pub fn artifact(&self) -> &ValidatedCommandH1Artifact {
        &self.authority.artifact
    }
    /// Returns non-fatal authenticated provider diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.authority.diagnostics
    }

    pub(crate) fn approved_policy(&self) -> CommandH1HostPolicy {
        self.authority.policy.clone()
    }

    /// Consumes the preparation and executes one fresh instance under the same current approval.
    ///
    /// # Errors
    /// Rejects substituted or revoked policy and stale source, composition, artifact or input
    /// before constructing an engine. Execution failures are returned as typed observations.
    pub fn execute(
        self,
        policy: &CommandH1HostPolicy,
    ) -> Result<ExecutedCommandH1, Vec<Diagnostic>> {
        if !self.authority.policy.same(policy) {
            return Err(vec![invalid()]);
        }
        self.authority.revalidate()?;
        let record = session::execute(Arc::clone(&self.authority));
        Ok(ExecutedCommandH1 { authority: self.authority, record })
    }

    #[cfg(test)]
    fn execute_before_call(
        self,
        policy: &CommandH1HostPolicy,
        before_call: &dyn Fn(),
    ) -> Result<ExecutedCommandH1, Vec<Diagnostic>> {
        if !self.authority.policy.same(policy) {
            return Err(vec![invalid()]);
        }
        self.authority.revalidate()?;
        let record = session::execute_with_before_call(Arc::clone(&self.authority), before_call);
        Ok(ExecutedCommandH1 { authority: self.authority, record })
    }
}

/// A consumed run retaining its private input authority through the eventual manifest commit.
/// This type cannot be manufactured from an editable execution record or component bytes.
pub struct ExecutedCommandH1 {
    authority: Arc<Authority>,
    record: CommandH1ExecutionRecord,
}

impl ExecutedCommandH1 {
    /// Returns the actual non-secret execution and teardown observations.
    #[must_use]
    pub const fn record(&self) -> &CommandH1ExecutionRecord {
        &self.record
    }
    /// Returns the exact artifact retained from preparation through execution.
    #[must_use]
    pub fn artifact(&self) -> &ValidatedCommandH1Artifact {
        &self.authority.artifact
    }
}

fn invalid() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-C4102",
        None,
        "Command source, grant, composition or current host approval does not match.",
        "Prepare one command from authenticated source and its matching private root-approved request.",
    )
}
