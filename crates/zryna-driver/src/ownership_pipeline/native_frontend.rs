//! Native M3 build keeps the reviewed source owner alive through preparation and publication.

use super::{
    CommandFailure, CommandFailureKind, DataOwnershipBuildRequest, DataOwnershipCandidateSuccess,
    ScalarValue, WorkspaceSourceRoot, closure_failure, failure, preparation, validate_request,
};
use crate::native_frontend::NativeBuildRequest;
use crate::{ArtifactOutputRoot, PublishedOwnershipBundle, capture_native_workspace_sources};
use std::path::PathBuf;
use zryna_diagnostics::Diagnostic;
use zryna_source::NormalizedSourcePath;

type Revalidate<'a> = &'a dyn Fn() -> Result<(), CommandFailure>;

pub(crate) fn build(
    input: &NativeBuildRequest,
) -> Result<PublishedOwnershipBundle, CommandFailure> {
    let request = DataOwnershipBuildRequest {
        workspace_root: input.workspace_root.clone(),
        entrypoint: input.entrypoint.clone(),
        artifact_stem: input.artifact_stem.clone(),
        targets: input.targets,
        // No Node authority is issued: this compatibility field is unused by build preparation.
        node_runtime: PathBuf::new(),
    };
    with_prepared(&request, None, validate_request, |success, revalidate| {
        crate::ownership_publication::publish_with_checkpoint(success, &|_| revalidate())
    })
}

// Match the existing isolated conformance route's request-shape validation. Production BUILD
// always uses full workspace validation above; all retained source checks are shared below.
#[cfg(test)]
pub(crate) fn run_for_test(
    request: &DataOwnershipBuildRequest,
    invocation: (String, Vec<ScalarValue>),
    consume: impl FnOnce(
        &DataOwnershipCandidateSuccess,
        Revalidate<'_>,
    ) -> Result<PublishedOwnershipBundle, CommandFailure>,
) -> Result<PublishedOwnershipBundle, CommandFailure> {
    with_prepared(request, Some(invocation), super::validate_request_shape, consume)
}

fn with_prepared(
    request: &DataOwnershipBuildRequest,
    invocation: Option<(String, Vec<ScalarValue>)>,
    validate: fn(&DataOwnershipBuildRequest) -> Result<(), CommandFailure>,
    consume: impl FnOnce(
        &DataOwnershipCandidateSuccess,
        Revalidate<'_>,
    ) -> Result<PublishedOwnershipBundle, CommandFailure>,
) -> Result<PublishedOwnershipBundle, CommandFailure> {
    validate(request)?;
    let root = WorkspaceSourceRoot::capture(&request.workspace_root)
        .map_err(|item| failure(CommandFailureKind::Source, item))?;
    // Hold workspace identity first, but create the exact compiler-owned directories before
    // discovery records directory state. Publication must not invalidate its own source owner.
    let output = ArtifactOutputRoot::prepare_for_workspace(&request.workspace_root)
        .map_err(|item| failure(CommandFailureKind::Preparation, item))?;
    let path = NormalizedSourcePath::new(request.entrypoint.clone()).map_err(|error| {
        failure(CommandFailureKind::Request, Diagnostic::from_source_error(&error))
    })?;
    let snapshot = capture_native_workspace_sources(&root, path)
        .and_then(crate::NativeSourceSnapshot::verify_v4)
        .map_err(|error| closure_failure(&error))?;
    let retained = snapshot.closure();
    // Existing preparation owns its closure. Re-seal an owned syntax value against the same
    // immutable source-map identity and authenticated records; never reopen or rediscover sources.
    // The original snapshot stays alive and is revalidated at every backend/publication phase.
    let tokens = zryna_frontend::native_lexer::lex(retained.sources())
        .map_err(|error| failure(CommandFailureKind::Source, error.diagnostic().clone()))?;
    let raw = zryna_frontend::native_parser::v4::parse_v4_candidate(retained.sources(), &tokens)
        .map_err(|error| failure(CommandFailureKind::Source, error.diagnostic().clone()))?;
    let closure = crate::ownership_closure::seal_native_closure(
        retained.entrypoint().clone(),
        retained.sources().clone(),
        raw,
        retained.modules(),
        retained.edges(),
        *retained.graph_sha256(),
    )
    .map_err(|error| closure_failure(&error))?;
    let revalidate = || {
        snapshot.revalidate().map_err(|error| closure_failure(&error))?;
        output.revalidate().map_err(|item| failure(CommandFailureKind::Preparation, item))
    };
    revalidate()?;
    let success =
        preparation::prepare_closure(request, closure, invocation, &|_| revalidate(), &revalidate)?;
    revalidate()?;
    consume(&success, &revalidate)
}
