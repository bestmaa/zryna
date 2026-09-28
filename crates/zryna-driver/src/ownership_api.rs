//! Public library surface for the internal `DataOwnershipV1` candidate.

pub use crate::native::{
    DataOwnershipExecutableIdentity, PreparedDataOwnershipExecutable,
    prepare_data_ownership_executable, publish_data_ownership_executable,
    publish_data_ownership_object,
};
pub use crate::ownership_closure::{
    VerifiedOwnershipModuleClosure, discover_ownership_module_closure,
    discover_ownership_module_closure_with_overlays,
};
pub use crate::ownership_commands::{build_data_ownership_candidate, run_data_ownership_candidate};
pub use crate::ownership_manifest::{
    MAX_OWNERSHIP_MANIFEST_BYTES, OWNERSHIP_MANIFEST_NAME, OwnershipManifestResult,
    OwnershipManifestV3, OwnershipTarget, OwnershipTraceEvent, OwnershipValueKind,
    decode_ownership_manifest_v3,
};
pub use crate::ownership_pipeline::{
    DATA_OWNERSHIP_CANDIDATE_PROFILE, DataOwnershipBuildRequest, DataOwnershipRunRequest,
};
pub use crate::ownership_publication::{PublishedOwnershipArtifact, PublishedOwnershipBundle};
