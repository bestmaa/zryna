//! Pending creation ownership and transition to the fixed executable inventory.

#[cfg(test)]
use std::io::{Seek, SeekFrom};
use std::{
    collections::BTreeMap,
    env,
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, File};
use same_file::Handle;
use zryna_diagnostics::Diagnostic;

use super::super::capture::{CapturedFile, capture_absolute};
use super::{
    MODULES, OLD, OLD_LIB, ROOT, RetainedDirectory, RetainedFile, SCOPE, ToolingStage, WRAPPER,
    WRAPPER_LIB, configure_create, configure_read, directory_identity, execution_error,
    hash_handle, metadata_is_link_or_reparse, open_regular, same_file_state, stage_changed,
    with_cleanup,
};

const MAX_STAGE_NAME_ATTEMPTS: u64 = 64;
static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

pub(super) fn create_root() -> Result<ToolingStage, Diagnostic> {
    create_root_in(&env::temp_dir())
}

pub(super) fn create_root_in(temp: &Path) -> Result<ToolingStage, Diagnostic> {
    if !temp.is_absolute() {
        return Err(stage_changed());
    }
    let parent = capture_absolute(temp)?;
    for _ in 0..MAX_STAGE_NAME_ATTEMPTS {
        let name = format!(
            ".zryna-tooling-{}-{}",
            std::process::id(),
            NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
        );
        match create_private_directory(&parent, &name) {
            Ok(()) => {
                let path = temp.join(&name);
                let mut stage = ToolingStage {
                    worker: path.join("worker.mjs"),
                    working_directory: path.clone(),
                    path,
                    root_parent: parent,
                    directories: BTreeMap::from([(ROOT, RetainedDirectory::pending(None, name))]),
                    files: BTreeMap::new(),
                    cleanup_attempted: false,
                };
                let result = (|| {
                    #[cfg(test)]
                    super::creation_tests::checkpoint("root-created")?;
                    let root = stage.directories.get_mut(ROOT).ok_or_else(stage_changed)?;
                    root.original = Some(
                        stage
                            .root_parent
                            .open_dir_nofollow(&root.name)
                            .map_err(|_| stage_changed())?,
                    );
                    #[cfg(test)]
                    super::creation_tests::checkpoint("root-opened")?;
                    seal_directory(root)
                })();
                return match result {
                    Ok(()) => Ok(stage),
                    Err(primary) => Err(with_cleanup(primary, stage.abort())),
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(stage_changed()),
        }
    }
    Err(execution_error("tooling private-stage name budget was exhausted"))
}

fn create_private_directory(parent: &Dir, name: &str) -> std::io::Result<()> {
    let builder = cap_std::fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use cap_std::fs::DirBuilderExt as _;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    parent.create_dir_with(name, &builder)
}

pub(super) fn create_directory(
    stage: &mut ToolingStage,
    parent_key: &'static str,
    name: &'static str,
    key: &'static str,
) -> Result<(), Diagnostic> {
    if !matches!(
        (parent_key, name, key),
        (ROOT, "node_modules", MODULES)
            | (MODULES, "@typescript", SCOPE)
            | (SCOPE, "typescript6", WRAPPER)
            | (WRAPPER, "lib", WRAPPER_LIB)
            | (SCOPE, "old", OLD)
            | (OLD, "lib", OLD_LIB)
    ) || stage.directories.contains_key(key)
    {
        return Err(stage_changed());
    }
    let parent = stage.bound_directory(parent_key)?;
    create_private_directory(&parent, name).map_err(|_| stage_changed())?;
    stage.directories.insert(key, RetainedDirectory::pending(Some(parent_key), name.to_owned()));
    #[cfg(test)]
    super::creation_tests::checkpoint("directory-created")?;
    let retained = stage.directories.get_mut(key).ok_or_else(stage_changed)?;
    retained.original = Some(parent.open_dir_nofollow(name).map_err(|_| stage_changed())?);
    #[cfg(test)]
    super::creation_tests::checkpoint("directory-opened")?;
    seal_directory(retained)
}

fn seal_directory(directory: &mut RetainedDirectory) -> Result<(), Diagnostic> {
    directory.identity = Some(directory_identity(directory.directory()?)?);
    #[cfg(test)]
    super::creation_tests::checkpoint("directory-identity")?;
    let state = directory
        .identity
        .as_ref()
        .ok_or_else(stage_changed)?
        .as_file()
        .metadata()
        .map_err(|_| stage_changed())?;
    #[cfg(test)]
    super::creation_tests::checkpoint("directory-metadata")?;
    if !state.is_dir() || metadata_is_link_or_reparse(&state) {
        return Err(stage_changed());
    }
    directory.state = Some(state);
    Ok(())
}

pub(super) fn seal_directory_states(
    directories: &mut BTreeMap<&'static str, RetainedDirectory>,
) -> Result<(), Diagnostic> {
    for directory in directories.values_mut() {
        directory.state = Some(
            directory
                .identity
                .as_ref()
                .ok_or_else(stage_changed)?
                .as_file()
                .metadata()
                .map_err(|_| stage_changed())?,
        );
    }
    Ok(())
}

pub(super) fn stage_file(
    stage: &mut ToolingStage,
    parent_key: &'static str,
    name: &'static str,
    captured: &CapturedFile,
) -> Result<(), Diagnostic> {
    let key = file_key(parent_key, name)?;
    if stage.files.contains_key(key) {
        return Err(stage_changed());
    }
    let parent = stage.bound_directory(parent_key)?;
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true).write(true).create_new(true).follow(FollowSymlinks::No);
    configure_create(&mut options);
    let original = parent.open_with(name, &options).map_err(|_| stage_changed())?;
    stage.files.insert(
        key,
        RetainedFile {
            parent: parent_key,
            name,
            original,
            identity: None,
            state: None,
            sha256: captured.sha256,
        },
    );
    #[cfg(test)]
    super::creation_tests::checkpoint("file-created")?;
    let retained = stage.files.get_mut(key).ok_or_else(stage_changed)?;
    #[cfg(test)]
    {
        retained
            .original
            .write_all(&captured.bytes[..captured.bytes.len().min(1)])
            .map_err(|_| stage_changed())?;
        super::creation_tests::checkpoint("file-prefix-written")?;
        retained.original.seek(SeekFrom::Start(0)).map_err(|_| stage_changed())?;
    }
    retained.original.write_all(&captured.bytes).map_err(|_| stage_changed())?;
    #[cfg(test)]
    super::creation_tests::checkpoint("file-written")?;
    retained.original.flush().map_err(|_| stage_changed())?;
    #[cfg(test)]
    super::creation_tests::checkpoint("file-flushed")?;
    retained.original.sync_all().map_err(|_| stage_changed())?;
    #[cfg(test)]
    super::creation_tests::checkpoint("file-synced")?;
    seal_file(retained, &parent, captured.bytes.len())
}

fn seal_file(
    file: &mut RetainedFile,
    parent: &Dir,
    expected_bytes: usize,
) -> Result<(), Diagnostic> {
    let original = original_identity(&file.original)?;
    #[cfg(test)]
    super::creation_tests::checkpoint("file-identity")?;
    let state = original.as_file().metadata().map_err(|_| stage_changed())?;
    #[cfg(test)]
    super::creation_tests::checkpoint("file-metadata")?;
    if !state.is_file()
        || metadata_is_link_or_reparse(&state)
        || state.len() != u64::try_from(expected_bytes).unwrap_or(u64::MAX)
    {
        return Err(stage_changed());
    }
    // Acquire a read-only handle to the same object before releasing any writer handle.
    let readonly = open_owned_regular(parent, file.name)?;
    let mut current = original_identity(&readonly)?;
    if current != original
        || hash_handle(&mut current)? != file.sha256
        || !same_file_state(&state, &current.as_file().metadata().map_err(|_| stage_changed())?)
    {
        return Err(stage_changed());
    }
    #[cfg(test)]
    super::creation_tests::checkpoint("file-readback")?;
    drop(current);
    let writer = std::mem::replace(&mut file.original, readonly);
    #[cfg(test)]
    super::creation_tests::checkpoint("file-readonly-retained")?;
    drop(original);
    drop(writer);
    let identity = open_regular(parent, file.name)?;
    if identity != original_identity(&file.original)? {
        return Err(stage_changed());
    }
    file.state = Some(identity.as_file().metadata().map_err(|_| stage_changed())?);
    file.identity = Some(identity);
    Ok(())
}

pub(super) fn original_identity(file: &File) -> Result<Handle, Diagnostic> {
    file.try_clone().map(File::into_std).and_then(Handle::from_file).map_err(|_| stage_changed())
}

pub(super) fn open_owned_regular(parent: &Dir, name: &str) -> Result<File, Diagnostic> {
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    configure_read(&mut options);
    #[cfg(windows)]
    {
        use cap_std::fs::OpenOptionsExt as _;
        options.share_mode(1 | 2 | 4);
    }
    parent.open_with(name, &options).map_err(|_| stage_changed())
}

fn file_key(parent: &str, name: &str) -> Result<&'static str, Diagnostic> {
    match (parent, name) {
        (ROOT, "worker.mjs") => Ok("worker"),
        (ROOT, "worker-v3.mjs") => Ok("worker-v3"),
        (ROOT, "limits-v3.mjs") => Ok("limits-v3"),
        (ROOT, "worker-v4.mjs") => Ok("worker-v4"),
        (ROOT, "limits-v4.mjs") => Ok("limits-v4"),
        (WRAPPER, "package.json") => Ok("wrapper-manifest"),
        (WRAPPER_LIB, "typescript.js") => Ok("wrapper-runtime"),
        (OLD, "package.json") => Ok("old-manifest"),
        (OLD_LIB, "typescript.js") => Ok("old-runtime"),
        _ => Err(stage_changed()),
    }
}
