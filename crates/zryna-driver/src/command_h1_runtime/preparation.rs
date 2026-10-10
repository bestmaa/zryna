//! The real provider-to-syntax-to-semantics path is mandatory for driver execution.

use super::{Authority, CommandH1HostPolicy, PreparedCommandH1, invalid};
use crate::{
    SourceToIrError, command_request::CapturedRequest, profile_composition::CommandH1Composition,
};
use std::{path::Path, sync::Arc};
use zryna_backend_webassembly::{ValidatedCommandH1Artifact, WitSource};
use zryna_frontend::VerifiedFrontendProviderV4;
use zryna_source::SourceMap;

/// Authenticates source, lowers it through real command semantics, and captures one private input.
/// The request path is never supplied to the guest or retained in an execution record.
///
/// # Errors
/// Rejects unsupported source, foreign witnesses or WIT, absent/extra/mismatched input,
/// missing root approval, revoked policy and any independent final-byte audit failure.
pub fn prepare_command_h1<Provider: VerifiedFrontendProviderV4 + ?Sized>(
    frontend: &Provider,
    sources: &SourceMap,
    wit: &[WitSource],
    request_path: Option<&Path>,
    policy: &CommandH1HostPolicy,
) -> Result<PreparedCommandH1, SourceToIrError> {
    prepare(frontend, sources, wit, request_path, Some(policy))
}

pub(crate) fn prepare_approved_request<Provider: VerifiedFrontendProviderV4 + ?Sized>(
    frontend: &Provider,
    sources: &SourceMap,
    wit: &[WitSource],
    request_path: Option<&Path>,
) -> Result<PreparedCommandH1, SourceToIrError> {
    prepare(frontend, sources, wit, request_path, None)
}

fn prepare<Provider: VerifiedFrontendProviderV4 + ?Sized>(
    frontend: &Provider,
    sources: &SourceMap,
    wit: &[WitSource],
    request_path: Option<&Path>,
    policy: Option<&CommandH1HostPolicy>,
) -> Result<PreparedCommandH1, SourceToIrError> {
    let syntax = frontend.analyze_verified_v4(sources).map_err(SourceToIrError::Frontend)?;
    let diagnostics = syntax.diagnostics().to_vec();
    let source = zryna_syntax::command_h1_v1::admit(&syntax, sources)
        .map_err(|error| SourceToIrError::Rejected(vec![error]))?;
    let compiled = zryna_semantics::command_h1_v1::lower(&source, sources)
        .map_err(SourceToIrError::Rejected)?;
    let required_key =
        source.environment().map(zryna_syntax::command_h1_v1::EnvironmentRequirement::key);
    if policy.is_some_and(|policy| !policy.active() || policy.key() != required_key)
        || request_path.is_some() != required_key.is_some()
    {
        return Err(SourceToIrError::Rejected(vec![invalid()]));
    }
    let request = request_path
        .map(|path| CapturedRequest::capture(path, required_key))
        .transpose()
        .map_err(|error| SourceToIrError::Rejected(vec![error]))?;
    // An explicit CLI request approves only the key actually admitted from that private file.
    // Capture happens once; no pathname read is used to manufacture a second input authority.
    let policy = match policy {
        Some(policy) => policy.clone(),
        None => match &request {
            Some(request) => CommandH1HostPolicy::environment(request.request().key())
                .map_err(|error| SourceToIrError::Rejected(vec![error]))?,
            None => CommandH1HostPolicy::deny_all(),
        },
    };
    let artifact = ValidatedCommandH1Artifact::emit(
        compiled.verified_ir(),
        compiled.runtime_abi(),
        sources,
        wit,
    )
    .map_err(|error| SourceToIrError::Rejected(vec![error]))?;
    let composition = CommandH1Composition::admit(&compiled, &artifact, sources, policy.key())
        .map_err(SourceToIrError::Rejected)?;
    let authority = Authority {
        sources: sources.clone(),
        compiled,
        artifact,
        wit: wit.to_vec(),
        composition,
        policy,
        request,
        diagnostics,
    };
    authority.revalidate().map_err(SourceToIrError::Rejected)?;
    Ok(PreparedCommandH1 { authority: Arc::new(authority) })
}
