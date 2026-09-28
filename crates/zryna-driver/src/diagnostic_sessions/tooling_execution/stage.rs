use std::{
    collections::BTreeMap,
    env, fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::Dir;
use same_file::Handle;
use sha2::{Digest, Sha256};
use zryna_diagnostics::Diagnostic;

use super::{capture::CapturedToolingClosure, execution_error};

mod inventory;

use inventory::{file_name, validate_inventory};

const MAX_STAGE_NAME_ATTEMPTS: u64 = 64;
static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

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
    name: &'static str,
    dir: Dir,
    identity: Handle,
    state: fs::Metadata,
}

#[derive(Debug)]
struct RetainedFile {
    parent: &'static str,
    name: &'static str,
    identity: Handle,
    state: fs::Metadata,
    sha256: [u8; 32],
}

/// Fixed nine-file stage. Unix owner permissions and Windows inherited private ACLs are trusted.
#[derive(Debug)]
pub(super) struct ToolingStage {
    path: PathBuf,
    worker: PathBuf,
    working_directory: PathBuf,
    directories: BTreeMap<&'static str, RetainedDirectory>,
    files: BTreeMap<&'static str, RetainedFile>,
}

impl ToolingStage {
    pub(super) fn create(captured: &CapturedToolingClosure) -> Result<Self, Diagnostic> {
        let (path, root) = create_root()?;
        let mut directories = BTreeMap::new();
        let retained = match retained_root(root) {
            Ok(retained) => retained,
            Err(error) => {
                let _ = fs::remove_dir(&path);
                return Err(error);
            }
        };
        directories.insert(ROOT, retained);
        let mut stage = Self {
            working_directory: path.clone(),
            worker: path.join("worker.mjs"),
            path,
            directories,
            files: BTreeMap::new(),
        };
        let prepared = (|| {
            create_directory(&mut stage.directories, ROOT, "node_modules", MODULES)?;
            create_directory(&mut stage.directories, MODULES, "@typescript", SCOPE)?;
            create_directory(&mut stage.directories, SCOPE, "typescript6", WRAPPER)?;
            create_directory(&mut stage.directories, WRAPPER, "lib", WRAPPER_LIB)?;
            create_directory(&mut stage.directories, SCOPE, "old", OLD)?;
            create_directory(&mut stage.directories, OLD, "lib", OLD_LIB)?;
            stage_file(&stage.directories, &mut stage.files, ROOT, "worker.mjs", &captured.worker)?;
            stage_file(
                &stage.directories,
                &mut stage.files,
                ROOT,
                "worker-v3.mjs",
                &captured.worker_v3,
            )?;
            stage_file(
                &stage.directories,
                &mut stage.files,
                ROOT,
                "limits-v3.mjs",
                &captured.limits_v3,
            )?;
            stage_file(
                &stage.directories,
                &mut stage.files,
                ROOT,
                "worker-v4.mjs",
                &captured.worker_v4,
            )?;
            stage_file(
                &stage.directories,
                &mut stage.files,
                ROOT,
                "limits-v4.mjs",
                &captured.limits_v4,
            )?;
            stage_file(
                &stage.directories,
                &mut stage.files,
                WRAPPER,
                "package.json",
                &captured.wrapper_manifest,
            )?;
            stage_file(
                &stage.directories,
                &mut stage.files,
                WRAPPER_LIB,
                "typescript.js",
                &captured.wrapper,
            )?;
            stage_file(
                &stage.directories,
                &mut stage.files,
                OLD,
                "package.json",
                &captured.typescript_manifest,
            )?;
            stage_file(
                &stage.directories,
                &mut stage.files,
                OLD_LIB,
                "typescript.js",
                &captured.typescript,
            )?;
            seal_directory_states(&mut stage.directories)?;
            #[cfg(target_os = "linux")]
            {
                stage.working_directory = capability_root(&stage.directories)?;
                stage.worker = stage.working_directory.join("worker.mjs");
            }
            stage.revalidate()
        })();
        if let Err(error) = prepared {
            stage.cleanup();
            return Err(error);
        }
        Ok(stage)
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

    pub(super) fn revalidate(&self) -> Result<(), Diagnostic> {
        for (key, directory) in &self.directories {
            let current = if *key == ROOT {
                validate_root_path(&self.path)?
            } else {
                let parent = directory
                    .parent
                    .and_then(|parent| self.directories.get(parent))
                    .ok_or_else(stage_changed)?;
                parent.dir.open_dir_nofollow(directory.name).map_err(|_| stage_changed())?
            };
            let identity = directory_identity(&current)?;
            let state = identity.as_file().metadata().map_err(|_| stage_changed())?;
            if identity != directory.identity
                || !same_file_state(&state, &directory.state)
                || !state.is_dir()
                || metadata_is_link_or_reparse(&state)
            {
                return Err(stage_changed());
            }
            validate_inventory(key, &directory.dir)?;
        }
        for file in self.files.values() {
            let parent = self.directories.get(file.parent).ok_or_else(stage_changed)?;
            let mut current = open_regular(&parent.dir, file.name)?;
            let state = current.as_file().metadata().map_err(|_| stage_changed())?;
            if current != file.identity
                || !same_file_state(&state, &file.state)
                || hash_handle(&mut current)? != file.sha256
            {
                return Err(stage_changed());
            }
        }
        Ok(())
    }

    fn cleanup(&mut self) {
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
            let Some(file) = self.files.remove(key) else { continue };
            let Some(parent) = self.directories.get(file.parent) else { return };
            let Ok(current) = open_regular(&parent.dir, file.name) else { return };
            if current != file.identity {
                return;
            }
            drop(current);
            drop(file);
            if parent.dir.remove_file(file_name(key)).is_err() {
                return;
            }
        }
        for key in [OLD_LIB, OLD, WRAPPER_LIB, WRAPPER, SCOPE, MODULES] {
            let Some(directory) = self.directories.remove(key) else { continue };
            let Some(parent_key) = directory.parent else { return };
            let Some(parent) = self.directories.get(parent_key) else { return };
            let Ok(current) = parent.dir.open_dir_nofollow(directory.name) else { return };
            let Ok(identity) = directory_identity(&current) else { return };
            if identity != directory.identity
                || current.entries().ok().and_then(|mut entries| entries.next()).is_some()
            {
                return;
            }
            drop(identity);
            drop(current);
            drop(directory);
            if parent.dir.remove_dir(key.rsplit('/').next().unwrap_or(key)).is_err() {
                return;
            }
        }
        let Some(root) = self.directories.remove(ROOT) else { return };
        if root.dir.entries().ok().and_then(|mut entries| entries.next()).is_some() {
            return;
        }
        drop(root);
        let _ = fs::remove_dir(&self.path);
    }
}

impl Drop for ToolingStage {
    fn drop(&mut self) {
        self.cleanup();
    }
}

fn create_root() -> Result<(PathBuf, Dir), Diagnostic> {
    for _ in 0..MAX_STAGE_NAME_ATTEMPTS {
        let sequence = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
        let path =
            env::temp_dir().join(format!(".zryna-tooling-{}-{sequence}", std::process::id()));
        match create_private_directory(&path) {
            Ok(()) => {
                let Ok(dir) = Dir::open_ambient_dir(&path, cap_std::ambient_authority()) else {
                    let _ = fs::remove_dir(&path);
                    return Err(stage_changed());
                };
                return Ok((path, dir));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(stage_changed()),
        }
    }
    Err(execution_error("tooling private-stage name budget was exhausted"))
}

#[cfg(unix)]
fn create_private_directory(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_directory(path: &Path) -> std::io::Result<()> {
    fs::create_dir(path)
}

fn retained_root(dir: Dir) -> Result<RetainedDirectory, Diagnostic> {
    let identity = directory_identity(&dir)?;
    let state = identity.as_file().metadata().map_err(|_| stage_changed())?;
    Ok(RetainedDirectory { parent: None, name: "", dir, identity, state })
}

fn create_directory(
    directories: &mut BTreeMap<&'static str, RetainedDirectory>,
    parent_key: &'static str,
    name: &'static str,
    key: &'static str,
) -> Result<(), Diagnostic> {
    let parent = directories.get(parent_key).ok_or_else(stage_changed)?;
    parent.dir.create_dir(name).map_err(|_| stage_changed())?;
    let dir = parent.dir.open_dir_nofollow(name).map_err(|_| stage_changed())?;
    let identity = directory_identity(&dir)?;
    let state = identity.as_file().metadata().map_err(|_| stage_changed())?;
    directories
        .insert(key, RetainedDirectory { parent: Some(parent_key), name, dir, identity, state });
    Ok(())
}

fn seal_directory_states(
    directories: &mut BTreeMap<&'static str, RetainedDirectory>,
) -> Result<(), Diagnostic> {
    for directory in directories.values_mut() {
        directory.state = directory.identity.as_file().metadata().map_err(|_| stage_changed())?;
    }
    Ok(())
}

fn stage_file(
    directories: &BTreeMap<&'static str, RetainedDirectory>,
    files: &mut BTreeMap<&'static str, RetainedFile>,
    parent_key: &'static str,
    name: &'static str,
    captured: &super::capture::CapturedFile,
) -> Result<(), Diagnostic> {
    let parent = directories.get(parent_key).ok_or_else(stage_changed)?;
    let mut options = cap_std::fs::OpenOptions::new();
    options.write(true).create_new(true).follow(FollowSymlinks::No);
    configure_create(&mut options);
    let mut output = parent.dir.open_with(name, &options).map_err(|_| stage_changed())?;
    output
        .write_all(&captured.bytes)
        .and_then(|()| output.flush())
        .and_then(|()| output.sync_all())
        .map_err(|_| stage_changed())?;
    drop(output);
    let identity = open_regular(&parent.dir, name)?;
    let state = identity.as_file().metadata().map_err(|_| stage_changed())?;
    if state.len() != u64::try_from(captured.bytes.len()).unwrap_or(u64::MAX) {
        return Err(stage_changed());
    }
    let key = match (parent_key, name) {
        (ROOT, "worker.mjs") => "worker",
        (ROOT, "worker-v3.mjs") => "worker-v3",
        (ROOT, "limits-v3.mjs") => "limits-v3",
        (ROOT, "worker-v4.mjs") => "worker-v4",
        (ROOT, "limits-v4.mjs") => "limits-v4",
        (WRAPPER, "package.json") => "wrapper-manifest",
        (WRAPPER_LIB, "typescript.js") => "wrapper-runtime",
        (OLD, "package.json") => "old-manifest",
        (OLD_LIB, "typescript.js") => "old-runtime",
        _ => return Err(stage_changed()),
    };
    files.insert(
        key,
        RetainedFile { parent: parent_key, name, identity, state, sha256: captured.sha256 },
    );
    Ok(())
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
        .map(cap_std::fs::File::into_std)
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
    Ok(PathBuf::from(format!("/proc/{}/fd/{}", std::process::id(), root.dir.as_raw_fd())))
}

#[cfg(unix)]
fn configure_create(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.mode(0o600);
}

#[cfg(not(unix))]
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
