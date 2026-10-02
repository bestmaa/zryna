use std::{
    ffi::OsStr,
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Component, Path},
};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::{ambient_authority, fs::Dir};
use same_file::Handle;
use sha2::{Digest, Sha256};
use zryna_diagnostics::Diagnostic;

use super::execution_error;

#[cfg(test)]
mod aggregate_tests;
mod pins;

use pins::{require_digest, validate_graph};

const MAX_WORKER_BYTES: usize = 128 * 1_024;
const MAX_MANIFEST_BYTES: usize = 8 * 1_024;
const MAX_WRAPPER_BYTES: usize = 1_024;
const MAX_TYPESCRIPT_BYTES: usize = 12 * 1_024 * 1_024;
const MAX_CLOSURE_BYTES: usize = 16 * 1_024 * 1_024;
const WRAPPER_MANIFEST_SHA256: &str =
    "d9b8fb53c67c947fece83bcb73fa8512227ea6a12b110e096fdab8ecdc02b655";
const WRAPPER_SHA256: &str = "d3f3cd2b04b7f466f4484df921b744223f7bd1f3e353ec9110bdf52695b983d5";
const TYPESCRIPT_MANIFEST_SHA256: &str =
    "9332e97c30d3e53ed54910b89207ed657fb444066484df6e5b6965bf130865e9";
const TYPESCRIPT_SHA256: &str = "569177652966bd528c319171c7dd22860dbf72bde116cbc4f644f1d02bb12e39";

const WORKER: &[&str] = &["adapters", "typescript-6", "src", "worker.mjs"];
const WORKER_V3: &[&str] = &["adapters", "typescript-6", "src", "worker-v3.mjs"];
const LIMITS_V3: &[&str] = &["adapters", "typescript-6", "src", "limits-v3.mjs"];
const WORKER_V4: &[&str] = &["adapters", "typescript-6", "src", "worker-v4.mjs"];
const LIMITS_V4: &[&str] = &["adapters", "typescript-6", "src", "limits-v4.mjs"];
const WRAPPER_MANIFEST: &[&str] = &[
    "node_modules",
    ".pnpm",
    "@typescript+typescript6@6.0.2",
    "node_modules",
    "@typescript",
    "typescript6",
    "package.json",
];
const WRAPPER: &[&str] = &[
    "node_modules",
    ".pnpm",
    "@typescript+typescript6@6.0.2",
    "node_modules",
    "@typescript",
    "typescript6",
    "lib",
    "typescript.js",
];
const TYPESCRIPT_MANIFEST: &[&str] =
    &["node_modules", ".pnpm", "typescript@6.0.3", "node_modules", "typescript", "package.json"];
const TYPESCRIPT: &[&str] = &[
    "node_modules",
    ".pnpm",
    "typescript@6.0.3",
    "node_modules",
    "typescript",
    "lib",
    "typescript.js",
];

pub(super) struct CapturedFile {
    pub(super) bytes: Vec<u8>,
    pub(super) sha256: [u8; 32],
}

pub(super) struct CapturedToolingClosure {
    pub(super) worker: CapturedFile,
    pub(super) worker_v3: CapturedFile,
    pub(super) limits_v3: CapturedFile,
    pub(super) worker_v4: CapturedFile,
    pub(super) limits_v4: CapturedFile,
    pub(super) wrapper_manifest: CapturedFile,
    pub(super) wrapper: CapturedFile,
    pub(super) typescript_manifest: CapturedFile,
    pub(super) typescript: CapturedFile,
}

impl CapturedToolingClosure {
    pub(super) fn capture_installed(root: &Path) -> Result<Self, Diagnostic> {
        if !root.is_absolute() {
            return Err(execution_error("installed tooling root must be absolute"));
        }
        let root = capture_absolute(root)?;
        let bootstrap = open_dir(&root, &["lib", "zryna", "bootstrap"])?;
        let worker = capture_file(&bootstrap, &["worker.mjs"], MAX_WORKER_BYTES)?;
        let expected_worker = include_bytes!("../../../../../adapters/typescript-6/src/worker.mjs");
        if worker.bytes != expected_worker {
            return Err(execution_error("installed worker differs from this tooling build"));
        }
        let worker_v3 = capture_file(&bootstrap, &["worker-v3.mjs"], MAX_WORKER_BYTES)?;
        let limits_v3 = capture_file(&bootstrap, &["limits-v3.mjs"], MAX_WORKER_BYTES)?;
        verify_v3_workers(&worker_v3, &limits_v3)?;
        let worker_v4 = capture_file(&bootstrap, &["worker-v4.mjs"], MAX_WORKER_BYTES)?;
        let limits_v4 = capture_file(&bootstrap, &["limits-v4.mjs"], MAX_WORKER_BYTES)?;
        verify_v4_workers(&worker_v4, &limits_v4)?;
        let wrapper_manifest = capture_file(
            &bootstrap,
            &["node_modules", "@typescript", "typescript6", "package.json"],
            MAX_MANIFEST_BYTES,
        )?;
        let wrapper = capture_file(
            &bootstrap,
            &["node_modules", "@typescript", "typescript6", "lib", "typescript.js"],
            MAX_WRAPPER_BYTES,
        )?;
        let typescript_manifest = capture_file(
            &bootstrap,
            &["node_modules", "@typescript", "old", "package.json"],
            MAX_MANIFEST_BYTES,
        )?;
        let typescript = capture_file(
            &bootstrap,
            &["node_modules", "@typescript", "old", "lib", "typescript.js"],
            MAX_TYPESCRIPT_BYTES,
        )?;
        require_digest(&wrapper_manifest, WRAPPER_MANIFEST_SHA256, "compatibility manifest")?;
        require_digest(&wrapper, WRAPPER_SHA256, "compatibility wrapper")?;
        require_digest(
            &typescript_manifest,
            TYPESCRIPT_MANIFEST_SHA256,
            "implementation manifest",
        )?;
        require_digest(&typescript, TYPESCRIPT_SHA256, "TypeScript runtime")?;
        validate_graph(&wrapper_manifest.bytes, &wrapper.bytes, &typescript_manifest.bytes)?;
        let captured = Self {
            worker,
            worker_v3,
            limits_v3,
            worker_v4,
            limits_v4,
            wrapper_manifest,
            wrapper,
            typescript_manifest,
            typescript,
        };
        captured.validate_size()?;
        Ok(captured)
    }

    pub(super) fn capture(root: &Path) -> Result<Self, Diagnostic> {
        if !root.is_absolute() {
            return Err(execution_error("tooling compiler root must be absolute"));
        }
        let root_path = root;
        let root = capture_absolute(root_path)?;
        authenticate_adapter_link(&root, root_path)?;
        let worker = capture_file(&root, WORKER, MAX_WORKER_BYTES)?;
        let worker_v3 = capture_file(&root, WORKER_V3, MAX_WORKER_BYTES)?;
        let limits_v3 = capture_file(&root, LIMITS_V3, MAX_WORKER_BYTES)?;
        verify_v3_workers(&worker_v3, &limits_v3)?;
        let worker_v4 = capture_file(&root, WORKER_V4, MAX_WORKER_BYTES)?;
        let limits_v4 = capture_file(&root, LIMITS_V4, MAX_WORKER_BYTES)?;
        verify_v4_workers(&worker_v4, &limits_v4)?;
        let wrapper_manifest = capture_file(&root, WRAPPER_MANIFEST, MAX_MANIFEST_BYTES)?;
        let wrapper = capture_file(&root, WRAPPER, MAX_WRAPPER_BYTES)?;
        let typescript_manifest = capture_file(&root, TYPESCRIPT_MANIFEST, MAX_MANIFEST_BYTES)?;
        let typescript = capture_file(&root, TYPESCRIPT, MAX_TYPESCRIPT_BYTES)?;
        validate_graph(&wrapper_manifest.bytes, &wrapper.bytes, &typescript_manifest.bytes)?;
        require_digest(
            &wrapper_manifest,
            WRAPPER_MANIFEST_SHA256,
            "TypeScript compatibility manifest",
        )?;
        require_digest(&wrapper, WRAPPER_SHA256, "TypeScript compatibility wrapper")?;
        require_digest(
            &typescript_manifest,
            TYPESCRIPT_MANIFEST_SHA256,
            "TypeScript implementation manifest",
        )?;
        require_digest(&typescript, TYPESCRIPT_SHA256, "TypeScript 6.0.3 runtime bundle")?;
        let captured = Self {
            worker,
            worker_v3,
            limits_v3,
            worker_v4,
            limits_v4,
            wrapper_manifest,
            wrapper,
            typescript_manifest,
            typescript,
        };
        captured.validate_size()?;
        Ok(captured)
    }

    pub(super) fn validate_size(&self) -> Result<(), Diagnostic> {
        validate_sizes([
            self.worker.bytes.len(),
            self.worker_v3.bytes.len(),
            self.limits_v3.bytes.len(),
            self.worker_v4.bytes.len(),
            self.limits_v4.bytes.len(),
            self.wrapper_manifest.bytes.len(),
            self.wrapper.bytes.len(),
            self.typescript_manifest.bytes.len(),
            self.typescript.bytes.len(),
        ])
    }
}

pub(super) fn validate_sizes(sizes: [usize; 9]) -> Result<(), Diagnostic> {
    let total = sizes
        .into_iter()
        .try_fold(0_usize, usize::checked_add)
        .ok_or_else(|| execution_error("tooling executable closure byte count overflowed"))?;
    if total > MAX_CLOSURE_BYTES {
        return Err(execution_error("tooling executable closure exceeds its byte limit"));
    }
    Ok(())
}

fn verify_v3_workers(worker: &CapturedFile, limits: &CapturedFile) -> Result<(), Diagnostic> {
    if worker.bytes != include_bytes!("../../../../../adapters/typescript-6/src/worker-v3.mjs")
        || limits.bytes != include_bytes!("../../../../../adapters/typescript-6/src/limits-v3.mjs")
    {
        return Err(execution_error("protocol-v3 tooling worker differs from this tooling build"));
    }
    Ok(())
}

fn verify_v4_workers(worker: &CapturedFile, limits: &CapturedFile) -> Result<(), Diagnostic> {
    if worker.bytes != include_bytes!("../../../../../adapters/typescript-6/src/worker-v4.mjs")
        || limits.bytes != include_bytes!("../../../../../adapters/typescript-6/src/limits-v4.mjs")
    {
        return Err(execution_error("protocol-v4 tooling worker differs from this tooling build"));
    }
    Ok(())
}

fn authenticate_adapter_link(root: &Dir, root_path: &Path) -> Result<(), Diagnostic> {
    let expected = open_dir(root, &WRAPPER_MANIFEST[..WRAPPER_MANIFEST.len() - 1])?;
    let adapter = root
        .open_dir("adapters")
        .and_then(|dir| dir.open_dir("typescript-6"))
        .and_then(|dir| dir.open_dir("node_modules"))
        .and_then(|dir| dir.open_dir("@typescript"))
        .map_err(|_| execution_error("TypeScript adapter dependency link is unavailable"))?;
    let metadata = adapter
        .symlink_metadata("typescript6")
        .map_err(|_| execution_error("TypeScript adapter dependency link is unavailable"))?;
    if cap_metadata_is_link_or_reparse(&metadata) {
        let linked_path =
            root_path.join("adapters/typescript-6/node_modules/@typescript/typescript6");
        let linked = Handle::from_path(&linked_path).map_err(|_| {
            execution_error("TypeScript adapter dependency link cannot be resolved")
        })?;
        if linked != directory_identity(&expected)? {
            return Err(execution_error(
                "TypeScript adapter dependency link resolved outside the pin",
            ));
        }
    } else {
        #[cfg(not(test))]
        return Err(execution_error(
            "TypeScript adapter dependency must be the pinned pnpm package link",
        ));
        #[cfg(test)]
        {
            let linked = adapter.open_dir_nofollow("typescript6").map_err(|_| {
                execution_error("TypeScript adapter dependency entry cannot be resolved")
            })?;
            let linked_state =
                directory_identity(&linked)?.as_file().metadata().map_err(|_| source_changed())?;
            if !linked_state.is_dir() || metadata_is_link_or_reparse(&linked_state) {
                return Err(source_changed());
            }
            let expected_manifest = open_regular(&expected, "package.json")?;
            let linked_manifest = open_regular(&linked, "package.json")?;
            let expected_lib = expected.open_dir_nofollow("lib").map_err(|_| source_changed())?;
            let linked_lib = linked.open_dir_nofollow("lib").map_err(|_| source_changed())?;
            let expected_runtime = open_regular(&expected_lib, "typescript.js")?;
            let linked_runtime = open_regular(&linked_lib, "typescript.js")?;
            if linked_manifest != expected_manifest || linked_runtime != expected_runtime {
                return Err(execution_error(
                    "TypeScript adapter dependency files are not the pinned package identities",
                ));
            }
        }
    }
    Ok(())
}

fn capture_file(root: &Dir, components: &[&str], limit: usize) -> Result<CapturedFile, Diagnostic> {
    let (name, directories) =
        components.split_last().ok_or_else(|| execution_error("tooling source path is empty"))?;
    let parent = open_dir(root, directories)?;
    let mut opened = open_regular(&parent, name)?;
    let metadata = opened.as_file().metadata().map_err(|_| source_changed())?;
    if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
        return Err(source_changed());
    }
    let bytes = bounded_read(&mut opened, limit)?;
    let reopened = open_regular(&parent, name)?;
    let current = reopened.as_file().metadata().map_err(|_| source_changed())?;
    let held = opened.as_file().metadata().map_err(|_| source_changed())?;
    if reopened != opened
        || !same_file_state(&metadata, &current)
        || !same_file_state(&metadata, &held)
    {
        return Err(source_changed());
    }
    Ok(CapturedFile { sha256: Sha256::digest(&bytes).into(), bytes })
}

fn open_dir(root: &Dir, components: &[&str]) -> Result<Dir, Diagnostic> {
    let mut current = root.try_clone().map_err(|_| source_changed())?;
    for component in components {
        let child = current.open_dir_nofollow(component).map_err(|_| source_changed())?;
        let reopened = current.open_dir_nofollow(component).map_err(|_| source_changed())?;
        if directory_identity(&child)? != directory_identity(&reopened)? {
            return Err(source_changed());
        }
        current = child;
    }
    Ok(current)
}

fn open_regular(parent: &Dir, name: &str) -> Result<Handle, Diagnostic> {
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    configure_open(&mut options);
    parent
        .open_with(OsStr::new(name), &options)
        .map(cap_std::fs::File::into_std)
        .and_then(Handle::from_file)
        .map_err(|_| source_changed())
}

fn bounded_read(handle: &mut Handle, limit: usize) -> Result<Vec<u8>, Diagnostic> {
    let length = handle.as_file().metadata().map_err(|_| source_changed())?.len();
    if length > u64::try_from(limit).unwrap_or(u64::MAX) {
        return Err(execution_error("tooling executable source exceeds its byte limit"));
    }
    handle.as_file_mut().seek(SeekFrom::Start(0)).map_err(|_| source_changed())?;
    let mut bytes = Vec::with_capacity(usize::try_from(length).unwrap_or(limit).min(limit));
    handle
        .as_file_mut()
        .take(u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| source_changed())?;
    if bytes.len() > limit {
        Err(execution_error("tooling executable source exceeds its byte limit"))
    } else {
        Ok(bytes)
    }
}

pub(super) fn capture_absolute(path: &Path) -> Result<Dir, Diagnostic> {
    #[cfg(unix)]
    let (mut current, mut components) = (
        Dir::open_ambient_dir(Path::new("/"), ambient_authority()).map_err(|_| source_changed())?,
        path.components(),
    );
    #[cfg(windows)]
    let (mut current, mut components) = windows_anchor(path)?;
    #[cfg(not(any(unix, windows)))]
    return Err(execution_error("tooling execution is unsupported on this platform"));
    for component in &mut components {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => {
                let child = current.open_dir_nofollow(name).map_err(|_| source_changed())?;
                let reopened = current.open_dir_nofollow(name).map_err(|_| source_changed())?;
                if directory_identity(&child)? != directory_identity(&reopened)? {
                    return Err(source_changed());
                }
                current = child;
            }
            Component::Prefix(_) if cfg!(windows) => {}
            Component::ParentDir | Component::Prefix(_) => return Err(source_changed()),
        }
    }
    Ok(current)
}

#[cfg(windows)]
fn windows_anchor(path: &Path) -> Result<(Dir, std::path::Components<'_>), Diagnostic> {
    use std::path::{PathBuf, Prefix};
    let mut components = path.components();
    let drive = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
            _ => return Err(source_changed()),
        },
        _ => return Err(source_changed()),
    };
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(source_changed());
    }
    let anchor = Dir::open_ambient_dir(
        PathBuf::from(format!("{}:\\", char::from(drive))),
        ambient_authority(),
    )
    .map_err(|_| source_changed())?;
    Ok((anchor, components))
}

fn directory_identity(directory: &Dir) -> Result<Handle, Diagnostic> {
    directory
        .try_clone()
        .map(Dir::into_std_file)
        .and_then(Handle::from_file)
        .map_err(|_| source_changed())
}

fn source_changed() -> Diagnostic {
    execution_error("tooling executable source is unavailable, linked, or changed during capture")
}

#[cfg(unix)]
fn configure_open(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.custom_flags(libc::O_NONBLOCK);
}

#[cfg(windows)]
fn configure_open(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    const FILE_SHARE_READ: u32 = 1;
    options.share_mode(FILE_SHARE_READ);
}

#[cfg(not(any(unix, windows)))]
fn configure_open(_options: &mut cap_std::fs::OpenOptions) {}

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

#[cfg(windows)]
fn cap_metadata_is_link_or_reparse(metadata: &cap_std::fs::Metadata) -> bool {
    use cap_std::fs::MetadataExt as _;
    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x0400 != 0
}

#[cfg(not(windows))]
fn cap_metadata_is_link_or_reparse(metadata: &cap_std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(not(windows))]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}
