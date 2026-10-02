use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, File};
use same_file::Handle;
use sha2::{Digest, Sha256};
use zryna_diagnostics::Diagnostic;

use super::{capture::CapturedToolingClosure, execution_error};

mod creation;
#[cfg(test)]
mod creation_tests;
mod inventory;

use creation::{
    create_directory, create_root, open_owned_regular, original_identity, seal_directory_states,
    stage_file,
};
use inventory::validate_inventory;

const ROOT: &str = "";
const MODULES: &str = "node_modules";
const SCOPE: &str = "node_modules/@typescript";
const WRAPPER: &str = "node_modules/@typescript/typescript6";
const WRAPPER_LIB: &str = "node_modules/@typescript/typescript6/lib";
const OLD: &str = "node_modules/@typescript/old";
const OLD_LIB: &str = "node_modules/@typescript/old/lib";

#[derive(Debug)]
struct RetainedDirectory {
    parent: Option<&'static str>,
    name: String,
    original: Option<Dir>,
    identity: Option<Handle>,
    state: Option<fs::Metadata>,
}

impl RetainedDirectory {
    fn pending(parent: Option<&'static str>, name: String) -> Self {
        Self { parent, name, original: None, identity: None, state: None }
    }

    fn directory(&self) -> Result<&Dir, Diagnostic> {
        self.original.as_ref().ok_or_else(stage_changed)
    }
}

#[derive(Debug)]
struct RetainedFile {
    parent: &'static str,
    name: &'static str,
    original: File,
    identity: Option<Handle>,
    state: Option<fs::Metadata>,
    sha256: [u8; 32],
}

/// Fixed nine-file stage. Unix owner permissions and Windows inherited private ACLs are trusted.
#[derive(Debug)]
pub(super) struct ToolingStage {
    path: PathBuf,
    worker: PathBuf,
    working_directory: PathBuf,
    root_parent: Dir,
    directories: BTreeMap<&'static str, RetainedDirectory>,
    files: BTreeMap<&'static str, RetainedFile>,
    cleanup_attempted: bool,
}

impl ToolingStage {
    pub(super) fn create(captured: &CapturedToolingClosure) -> Result<Self, Diagnostic> {
        captured.validate_size()?;
        let mut stage = create_root()?;
        let prepared = (|| {
            create_directory(&mut stage, ROOT, "node_modules", MODULES)?;
            create_directory(&mut stage, MODULES, "@typescript", SCOPE)?;
            create_directory(&mut stage, SCOPE, "typescript6", WRAPPER)?;
            create_directory(&mut stage, WRAPPER, "lib", WRAPPER_LIB)?;
            create_directory(&mut stage, SCOPE, "old", OLD)?;
            create_directory(&mut stage, OLD, "lib", OLD_LIB)?;
            for (parent, name, file) in [
                (ROOT, "worker.mjs", &captured.worker),
                (ROOT, "worker-v3.mjs", &captured.worker_v3),
                (ROOT, "limits-v3.mjs", &captured.limits_v3),
                (ROOT, "worker-v4.mjs", &captured.worker_v4),
                (ROOT, "limits-v4.mjs", &captured.limits_v4),
                (WRAPPER, "package.json", &captured.wrapper_manifest),
                (WRAPPER_LIB, "typescript.js", &captured.wrapper),
                (OLD, "package.json", &captured.typescript_manifest),
                (OLD_LIB, "typescript.js", &captured.typescript),
            ] {
                stage_file(&mut stage, parent, name, file)?;
            }
            seal_directory_states(&mut stage.directories)?;
            #[cfg(target_os = "linux")]
            {
                stage.working_directory = capability_root(&stage.directories)?;
                stage.worker = stage.working_directory.join("worker.mjs");
            }
            stage.revalidate()
        })();
        match prepared {
            Ok(()) => Ok(stage),
            Err(primary) => Err(with_cleanup(primary, stage.abort())),
        }
    }

    pub(super) fn worker(&self) -> &Path {
        &self.worker
    }

    pub(super) fn worker_v3(&self) -> PathBuf {
        self.working_directory.join("worker-v3.mjs")
    }

    pub(super) fn worker_v4(&self) -> PathBuf {
        self.working_directory.join("worker-v4.mjs")
    }

    pub(super) fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    #[cfg(test)]
    pub(super) fn physical_path(&self) -> &Path {
        &self.path
    }

    fn bound_directory(&self, key: &'static str) -> Result<Dir, Diagnostic> {
        let retained = self.directories.get(key).ok_or_else(stage_changed)?;
        let current = match retained.parent {
            Some(parent) => self
                .bound_directory(parent)?
                .open_dir_nofollow(&retained.name)
                .map_err(|_| stage_changed())?,
            None => {
                self.root_parent.open_dir_nofollow(&retained.name).map_err(|_| stage_changed())?
            }
        };
        if directory_identity(&current)? != directory_identity(retained.directory()?)? {
            return Err(stage_changed());
        }
        Ok(current)
    }

    pub(super) fn revalidate(&self) -> Result<(), Diagnostic> {
        let root = self.directories.get(ROOT).ok_or_else(stage_changed)?;
        if directory_identity(&validate_root_path(&self.path)?)?
            != directory_identity(root.directory()?)?
        {
            return Err(stage_changed());
        }
        for (key, directory) in &self.directories {
            let current = self.bound_directory(key)?;
            let identity = directory_identity(&current)?;
            let state = identity.as_file().metadata().map_err(|_| stage_changed())?;
            if Some(&identity) != directory.identity.as_ref()
                || !same_file_state(&state, directory.state.as_ref().ok_or_else(stage_changed)?)
                || !state.is_dir()
                || metadata_is_link_or_reparse(&state)
            {
                return Err(stage_changed());
            }
            validate_inventory(key, directory.directory()?)?;
        }
        for file in self.files.values() {
            let parent = self.bound_directory(file.parent)?;
            let mut current = open_regular(&parent, file.name)?;
            let state = current.as_file().metadata().map_err(|_| stage_changed())?;
            if Some(&current) != file.identity.as_ref()
                || !same_file_state(&state, file.state.as_ref().ok_or_else(stage_changed)?)
                || hash_handle(&mut current)? != file.sha256
            {
                return Err(stage_changed());
            }
        }
        Ok(())
    }

    /// Consuming explicit cleanup. A failed cleanup is retained as an error, never retried by Drop.
    pub(super) fn abort(mut self) -> Result<(), Diagnostic> {
        self.cleanup_attempted = true;
        self.cleanup()
    }

    fn cleanup_file(&mut self, key: &'static str) -> Result<(), Diagnostic> {
        let file = self.files.get(key).ok_or_else(stage_changed)?;
        let parent = self.bound_directory(file.parent)?;
        let original = original_identity(&file.original)?;
        let current = original_identity(&open_owned_regular(&parent, file.name)?)?;
        if current != original {
            return Err(stage_changed());
        }
        let name = file.name;
        drop(current);
        drop(original);
        drop(self.files.remove(key));
        parent.remove_file(name).map_err(|_| stage_changed())
    }

    fn cleanup_directory(&mut self, key: &'static str) -> Result<(), Diagnostic> {
        let directory = self.directories.get(key).ok_or_else(stage_changed)?;
        let parent = match directory.parent {
            Some(parent) => self.bound_directory(parent)?,
            None => self.root_parent.try_clone().map_err(|_| stage_changed())?,
        };
        let current = self.bound_directory(key)?;
        if current
            .entries()
            .map_err(|_| stage_changed())?
            .next()
            .transpose()
            .map_err(|_| stage_changed())?
            .is_some()
        {
            return Err(stage_changed());
        }
        let name = directory.name.clone();
        drop(current);
        drop(self.directories.remove(key));
        parent.remove_dir(name).map_err(|_| stage_changed())
    }

    fn cleanup(&mut self) -> Result<(), Diagnostic> {
        let mut failures = Vec::new();
        for key in [
            "old-runtime",
            "old-manifest",
            "wrapper-runtime",
            "wrapper-manifest",
            "worker-v3",
            "limits-v3",
            "worker-v4",
            "limits-v4",
            "worker",
        ] {
            if self.files.contains_key(key)
                && let Err(error) = self.cleanup_file(key)
            {
                failures.push(format!("file {key}: {}", error.message()));
            }
        }
        for key in [OLD_LIB, OLD, WRAPPER_LIB, WRAPPER, SCOPE, MODULES, ROOT] {
            let name = self.directories.get(key).map(|directory| directory.name.clone());
            if self.directories.contains_key(key)
                && let Err(error) = self.cleanup_directory(key)
            {
                failures.push(format!("directory {key:?} ({name:?}): {}", error.message()));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(execution_error(format!(
                "tooling stage cleanup incomplete: {}",
                failures.join("; ")
            )))
        }
    }
}

impl Drop for ToolingStage {
    fn drop(&mut self) {
        if !self.cleanup_attempted {
            self.cleanup_attempted = true;
            let _ = self.cleanup();
        }
    }
}

fn with_cleanup(primary: Diagnostic, cleanup: Result<(), Diagnostic>) -> Diagnostic {
    match cleanup {
        Ok(()) => primary,
        Err(cleanup) => Diagnostic::error(
            primary.code(),
            primary.path().map(str::to_owned),
            format!("{}; cleanup [{}]: {}", primary.message(), cleanup.code(), cleanup.message()),
            primary.guidance(),
        ),
    }
}

fn validate_root_path(path: &Path) -> Result<Dir, Diagnostic> {
    let metadata = fs::symlink_metadata(path).map_err(|_| stage_changed())?;
    if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
        return Err(stage_changed());
    }
    Dir::open_ambient_dir(path, cap_std::ambient_authority()).map_err(|_| stage_changed())
}

fn open_regular(parent: &Dir, name: &str) -> Result<Handle, Diagnostic> {
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    configure_read(&mut options);
    parent
        .open_with(name, &options)
        .map(File::into_std)
        .and_then(Handle::from_file)
        .map_err(|_| stage_changed())
}

fn hash_handle(handle: &mut Handle) -> Result<[u8; 32], Diagnostic> {
    handle.as_file_mut().seek(SeekFrom::Start(0)).map_err(|_| stage_changed())?;
    let mut digest = Sha256::new();
    let mut limited = handle.as_file_mut().take(16 * 1_024 * 1_024 + 1);
    std::io::copy(&mut limited, &mut digest).map_err(|_| stage_changed())?;
    if limited.limit() == 0 {
        return Err(stage_changed());
    }
    Ok(digest.finalize().into())
}

fn directory_identity(directory: &Dir) -> Result<Handle, Diagnostic> {
    directory
        .try_clone()
        .map(Dir::into_std_file)
        .and_then(Handle::from_file)
        .map_err(|_| stage_changed())
}

#[cfg(target_os = "linux")]
fn capability_root(
    directories: &BTreeMap<&'static str, RetainedDirectory>,
) -> Result<PathBuf, Diagnostic> {
    use std::os::fd::AsRawFd as _;
    let root = directories.get(ROOT).ok_or_else(stage_changed)?;
    Ok(PathBuf::from(format!("/proc/{}/fd/{}", std::process::id(), root.directory()?.as_raw_fd())))
}

#[cfg(unix)]
fn configure_create(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.mode(0o600);
}

#[cfg(windows)]
fn configure_create(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.share_mode(1 | 4);
}

#[cfg(not(any(unix, windows)))]
fn configure_create(_options: &mut cap_std::fs::OpenOptions) {}

#[cfg(unix)]
fn configure_read(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.custom_flags(libc::O_NONBLOCK);
}

#[cfg(windows)]
fn configure_read(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.share_mode(1);
}

#[cfg(not(any(unix, windows)))]
fn configure_read(_options: &mut cap_std::fs::OpenOptions) {}

#[cfg(unix)]
fn same_file_state(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.size() == right.size()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

#[cfg(windows)]
fn same_file_state(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    left.file_attributes() == right.file_attributes()
        && left.creation_time() == right.creation_time()
        && left.last_write_time() == right.last_write_time()
        && left.file_size() == right.file_size()
}

#[cfg(not(any(unix, windows)))]
fn same_file_state(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

#[cfg(windows)]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x0400 != 0
}

#[cfg(not(windows))]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn stage_changed() -> Diagnostic {
    execution_error("tooling private-stage identity, inventory, or executable bytes changed")
}
