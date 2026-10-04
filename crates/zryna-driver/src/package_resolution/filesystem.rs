use std::{
    ffi::OsStr,
    fs,
    io::{self, Read as _, Seek as _, SeekFrom, Write as _},
    path::{Component, Path, PathBuf},
};

use cap_fs_ext::{DirExt as _, FollowSymlinks, OpenOptionsFollowExt as _};
use cap_std::{ambient_authority, fs::Dir};
use same_file::Handle;
use zryna_package::ResolveError;

pub(super) struct CapturedRoot {
    path: PathBuf,
    root: Dir,
    identity: Handle,
    metadata: fs::Metadata,
    _anchors: Vec<Dir>,
}

impl CapturedRoot {
    pub(super) fn capture(path: &Path) -> Result<Self, ResolveError> {
        if !path.is_absolute() {
            return Err(ResolveError::source("package source root must be absolute"));
        }
        let (anchors, root) = capture_absolute_root(path)
            .map_err(|_| ResolveError::source("package source root cannot be retained safely"))?;
        let identity = directory_identity(&root)
            .map_err(|_| ResolveError::source("package source root identity is unavailable"))?;
        let metadata = identity
            .as_file()
            .metadata()
            .map_err(|_| ResolveError::source("package source root metadata is unavailable"))?;
        Ok(Self { path: path.to_owned(), root, identity, metadata, _anchors: anchors })
    }

    pub(super) fn open_relative(&self, path: &str) -> Result<Dir, ResolveError> {
        let mut directory = self
            .root
            .try_clone()
            .map_err(|_| ResolveError::source("package root capability cannot be cloned"))?;
        for component in path.split('/') {
            directory = directory.open_dir_nofollow(component).map_err(|_| {
                ResolveError::source("package source directory is absent or unsafe")
            })?;
        }
        Ok(directory)
    }

    pub(super) fn revalidate(&self) -> Result<(), ResolveError> {
        let (_, reopened) = capture_absolute_root(&self.path)
            .map_err(|_| ResolveError::source("package source root cannot be revalidated"))?;
        let current = directory_identity(&self.root)
            .map_err(|_| ResolveError::source("package source root cannot be revalidated"))?;
        let reopened = directory_identity(&reopened)
            .map_err(|_| ResolveError::source("package source root cannot be revalidated"))?;
        let metadata = current
            .as_file()
            .metadata()
            .map_err(|_| ResolveError::source("package source root cannot be revalidated"))?;
        if current != self.identity
            || reopened != self.identity
            || !same_file_state(&self.metadata, &metadata)
        {
            return Err(ResolveError::source("package source root changed during resolution"));
        }
        Ok(())
    }
}

pub(super) fn ensure_same_directory(left: &Dir, right: &Dir) -> Result<(), ResolveError> {
    let left = directory_identity(left)
        .map_err(|_| ResolveError::source("root package directory cannot be revalidated"))?;
    let right = directory_identity(right)
        .map_err(|_| ResolveError::source("root package directory cannot be revalidated"))?;
    if left == right {
        Ok(())
    } else {
        Err(ResolveError::source("root package directory changed during resolution"))
    }
}

pub(super) struct RetainedFile {
    directory: Dir,
    name: String,
    pub(super) handle: Handle,
    metadata: fs::Metadata,
}

impl RetainedFile {
    pub(super) fn revalidate(&mut self) -> Result<(), ResolveError> {
        crate::source_identity::validate(&self.directory, &self.name, &self.handle, &self.metadata)
            .map_err(|_| ResolveError::source("package file changed during resolution"))
    }
}

pub(super) fn read_retained(
    directory: &Dir,
    name: &str,
    limit: usize,
) -> Result<(Vec<u8>, RetainedFile), ResolveError> {
    let (bytes, handle, metadata) = open_and_read(directory, name, limit)?;
    let directory = directory
        .try_clone()
        .map_err(|_| ResolveError::source("package directory capability cannot be retained"))?;
    crate::source_identity::validate(&directory, name, &handle, &metadata)
        .map_err(|_| ResolveError::source("package file changed during capture"))?;
    Ok((bytes, RetainedFile { directory, name: name.to_owned(), handle, metadata }))
}

fn open_and_read(
    directory: &Dir,
    name: &str,
    limit: usize,
) -> Result<(Vec<u8>, Handle, fs::Metadata), ResolveError> {
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    configure_final_open(&mut options);
    let mut handle = directory
        .open_with(name, &options)
        .map(cap_std::fs::File::into_std)
        .and_then(Handle::from_file)
        .map_err(|_| ResolveError::source("package file is absent or unsafe"))?;
    let metadata = handle
        .as_file()
        .metadata()
        .map_err(|_| ResolveError::source("package file metadata is unavailable"))?;
    if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
        return Err(ResolveError::source("package input is not a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if metadata.nlink() != 1 {
            return Err(ResolveError::source("package input has a hard-link alias"));
        }
    }
    if metadata.len() > u64::try_from(limit).unwrap_or(u64::MAX) {
        return Err(ResolveError::source("package file exceeds its byte limit"));
    }
    handle
        .as_file_mut()
        .seek(SeekFrom::Start(0))
        .map_err(|_| ResolveError::source("package file cannot be read"))?;
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(limit).min(limit));
    handle
        .as_file_mut()
        .take(u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| ResolveError::source("package file cannot be read"))?;
    if bytes.len() > limit {
        return Err(ResolveError::source("package file exceeds its byte limit"));
    }
    crate::source_identity::validate(directory, name, &handle, &metadata)
        .map_err(|_| ResolveError::source("package file changed during read"))?;
    Ok((bytes, handle, metadata))
}

pub(super) fn publish_lock(
    directory: &Dir,
    lock_name: &str,
    bytes: &[u8],
) -> Result<(), ResolveError> {
    let temporary = ".zryna.lock.pending";
    let mut options = cap_std::fs::OpenOptions::new();
    options.write(true).create_new(true).follow(FollowSymlinks::No);
    let mut file = directory
        .open_with(temporary, &options)
        .map_err(|_| ResolveError::publication("lock staging file cannot be created"))?;
    let outcome = (|| {
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| ResolveError::publication("lock staging file cannot be synchronized"))?;
        if let Ok(metadata) = directory.symlink_metadata(lock_name)
            && (!metadata.is_file() || metadata.is_symlink())
        {
            return Err(ResolveError::publication("existing lock path is not a regular file"));
        }
        directory
            .rename(temporary, directory, lock_name)
            .map_err(|_| ResolveError::publication("atomic lockfile publication failed"))
    })();
    if outcome.is_err() {
        let _ = directory.remove_file(temporary);
    }
    outcome
}

fn capture_child(parent: &Dir, name: &OsStr) -> io::Result<Dir> {
    let child = parent.open_dir_nofollow(name)?;
    let metadata = child.try_clone().map(Dir::into_std_file)?.metadata()?;
    if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
        return Err(io::Error::other("unsafe root component"));
    }
    Ok(child)
}

#[cfg(windows)]
fn capture_absolute_root(path: &Path) -> io::Result<(Vec<Dir>, Dir)> {
    use std::path::{PathBuf, Prefix};
    let mut components = path.components();
    let drive = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
            _ => return Err(io::Error::other("unsupported root")),
        },
        _ => return Err(io::Error::other("unsupported root")),
    };
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(io::Error::other("unsupported root"));
    }
    let anchor = Dir::open_ambient_dir(
        PathBuf::from(format!("{}:\\", char::from(drive))),
        ambient_authority(),
    )?;
    capture_components(anchor, components)
}

#[cfg(unix)]
fn capture_absolute_root(path: &Path) -> io::Result<(Vec<Dir>, Dir)> {
    capture_components(Dir::open_ambient_dir("/", ambient_authority())?, path.components())
}

fn capture_components<'a>(
    anchor: Dir,
    components: impl Iterator<Item = Component<'a>>,
) -> io::Result<(Vec<Dir>, Dir)> {
    let mut anchors = vec![anchor];
    for component in components {
        match component {
            Component::Normal(name) => {
                let child =
                    capture_child(anchors.last().ok_or_else(|| io::Error::other("root"))?, name)?;
                anchors.push(child);
            }
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) => return Err(io::Error::other("root")),
        }
    }
    let root = anchors.last().ok_or_else(|| io::Error::other("root"))?.try_clone()?;
    Ok((anchors, root))
}

fn directory_identity(directory: &Dir) -> io::Result<Handle> {
    directory.try_clone().map(Dir::into_std_file).and_then(Handle::from_file)
}

#[cfg(unix)]
fn configure_final_open(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.custom_flags(libc::O_NONBLOCK);
}

#[cfg(windows)]
fn configure_final_open(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.share_mode(1);
}

#[cfg(windows)]
pub(super) fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
pub(super) fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(unix)]
fn same_file_state(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    left.size() == right.size()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

#[cfg(windows)]
fn same_file_state(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    left.file_attributes() == right.file_attributes()
        && left.creation_time() == right.creation_time()
        && left.last_write_time() == right.last_write_time()
        && left.file_size() == right.file_size()
}
