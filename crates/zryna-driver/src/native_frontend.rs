//! Feature-gated native source-workspace builds over existing sealed backend/publication paths.
//! This API carries no installed-distribution or runtime-execution authority.

use crate::{CommandFailure, CommandSuccess, PublishedOwnershipBundle, TargetSelection};
use std::path::PathBuf;

/// The three existing source profiles admitted by the private build route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeBuildProfile {
    /// Existing M1 scalar profile.
    I32,
    /// Existing M2 control-flow profile.
    ControlFlow,
    /// Existing M3 data-ownership profile.
    DataOwnership,
}

/// Explicit source-workspace build; deliberately contains no Node runtime or run invocation.
#[derive(Clone, Debug)]
pub struct NativeBuildRequest {
    /// Absolute source-checkout workspace root.
    pub workspace_root: PathBuf,
    /// Portable workspace-relative source entrypoint.
    pub entrypoint: String,
    /// Validated create-only artifact stem.
    pub artifact_stem: String,
    /// Existing JavaScript, WebAssembly or native object targets.
    pub targets: TargetSelection,
    /// Existing semantic profile.
    pub profile: NativeBuildProfile,
}

/// Existing transaction results, without a new manifest or artifact format.
#[derive(Debug)]
pub enum NativeBuildSuccess {
    /// Existing manifest-v1 build result.
    Scalar(CommandSuccess),
    /// Existing manifest-v2 build result.
    ControlFlow(CommandSuccess),
    /// Existing manifest-v3 build result.
    Ownership(PublishedOwnershipBundle),
}

/// Prepares a native frontend build with mandatory source/syntax/semantic verification.
///
/// # Errors
/// Rejects unsupported targets, architecture/path violations, source changes, semantic errors,
/// backend failures and existing destinations. No default or installed route selects this API.
pub fn build_workspace(request: &NativeBuildRequest) -> Result<NativeBuildSuccess, CommandFailure> {
    if request.targets.component() {
        return Err(crate::pipeline::request_error(
            "ZRYNA-C1013",
            "the private native frontend build does not admit browser components",
            "select javascript, webassembly, native or all with an existing source profile",
        ));
    }
    match request.profile {
        NativeBuildProfile::I32 => {
            crate::pipeline::native_frontend::scalar(request).map(NativeBuildSuccess::Scalar)
        }
        NativeBuildProfile::ControlFlow => crate::pipeline::native_frontend::control_flow(request)
            .map(NativeBuildSuccess::ControlFlow),
        NativeBuildProfile::DataOwnership => {
            crate::ownership_pipeline::native_frontend::build(request)
                .map(NativeBuildSuccess::Ownership)
        }
    }
}
