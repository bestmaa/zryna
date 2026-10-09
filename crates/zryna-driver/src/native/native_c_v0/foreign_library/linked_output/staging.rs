use super::*;
use std::{fs, io::Read as _, os::unix::fs::OpenOptionsExt as _, path::Path};

pub(super) fn stages(root: &ArtifactOutputRoot, count: usize) -> Result<Vec<NativeStage>, Failure> {
    let mut stages = Vec::new();
    for _ in 0..count {
        match NativeStage::create(root, "linked-observation") {
            Ok(stage) => stages.push(stage),
            Err(error) => return finish(&stages, Err(error.into()), &[]),
        }
    }
    Ok(stages)
}

pub(super) fn finish<T>(
    stages: &[NativeStage],
    result: Result<T, Failure>,
    completed: &[Invocation],
) -> Result<T, Failure> {
    let cleanup = stages.iter().flat_map(NativeStage::cleanup).collect::<Vec<_>>();
    match result {
        Ok(value) if cleanup.is_empty() => Ok(value),
        Ok(_) => Err(Failure { diagnostics: cleanup, invocations: completed.to_vec() }),
        Err(mut failure) => {
            failure.diagnostics.extend(cleanup);
            Err(failure)
        }
    }
}

/// Bounded no-follow, opened-file and path identity checks; reads never grow beyond the budget.
pub(super) fn bytes(path: &Path, limit: usize) -> Result<Vec<u8>, Diagnostic> {
    let before = native::regular_file_identity(path).map_err(|()| rejected())?;
    if before.length == 0 || before.length > limit as u64 {
        return Err(rejected());
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| rejected())?;
    let opened = native::tool_identity_from_metadata(&file.metadata().map_err(|_| rejected())?)
        .map_err(|()| rejected())?;
    if opened != before {
        return Err(rejected());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes).map_err(|_| rejected())?;
    let after = native::regular_file_identity(path).map_err(|()| rejected())?;
    if before != after || bytes.len() as u64 != before.length {
        return Err(rejected());
    }
    Ok(bytes)
}
