//! Exact-handle identity, privacy and immutable file-state observations.

use std::{ffi::OsStr, fs::File, io};

use cap_std::fs::{Dir, Metadata, OpenOptions};

#[cfg(windows)]
pub(super) type PrivateFile = zryna_windows_filesystem::PrivateGrantFile;

#[cfg(not(windows))]
pub(super) struct PrivateFile {
    file: File,
    #[cfg(unix)]
    owner: u32,
}

#[cfg(not(windows))]
impl PrivateFile {
    pub(super) fn file(&self) -> &File {
        &self.file
    }
}

#[cfg(unix)]
pub(super) fn retain(file: File) -> io::Result<PrivateFile> {
    let retained = PrivateFile { file, owner: nix::unistd::geteuid().as_raw() };
    privacy(&retained, &Metadata::from_file(retained.file())?)?;
    Ok(retained)
}

#[cfg(windows)]
pub(super) fn retain(file: File) -> io::Result<PrivateFile> {
    zryna_windows_filesystem::admit_private_grant_file(file)
}

#[cfg(not(any(unix, windows)))]
pub(super) fn retain(_: File) -> io::Result<PrivateFile> {
    Err(invalid())
}

#[cfg(unix)]
pub(super) fn privacy(file: &PrivateFile, metadata: &Metadata) -> io::Result<()> {
    use cap_std::fs::MetadataExt as _;
    if !metadata.is_file()
        || linked(metadata)
        || metadata.uid() != file.owner
        || metadata.uid() != nix::unistd::geteuid().as_raw()
        || metadata.nlink() != 1
        || metadata.mode() & 0o7777 != 0o600
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn privacy(file: &PrivateFile, _: &Metadata) -> io::Result<()> {
    file.revalidate()
}

#[cfg(not(any(unix, windows)))]
pub(super) fn privacy(_: &PrivateFile, _: &Metadata) -> io::Result<()> {
    Err(invalid())
}

#[cfg(unix)]
pub(super) fn identity(metadata: &Metadata) -> io::Result<(u64, u64)> {
    use cap_std::fs::MetadataExt as _;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(invalid());
    }
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
pub(super) fn identity(metadata: &Metadata) -> io::Result<(u64, u64)> {
    use cap_primitives::fs::_WindowsByHandle as _;
    Ok((
        u64::from(metadata.volume_serial_number().ok_or_else(invalid)?),
        metadata.file_index().ok_or_else(invalid)?,
    ))
}

#[cfg(not(any(unix, windows)))]
pub(super) fn identity(_: &Metadata) -> io::Result<(u64, u64)> {
    Err(invalid())
}

#[cfg(unix)]
pub(super) fn linked(metadata: &Metadata) -> bool {
    metadata.is_symlink()
}

#[cfg(windows)]
pub(super) fn linked(metadata: &Metadata) -> bool {
    use cap_std::fs::MetadataExt as _;
    metadata.is_symlink() || metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(any(unix, windows)))]
pub(super) fn linked(_: &Metadata) -> bool {
    true
}

#[cfg(unix)]
pub(super) fn directory_options(options: &mut OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.custom_flags(libc::O_NONBLOCK | libc::O_DIRECTORY);
}

#[cfg(windows)]
pub(super) fn directory_options(options: &mut OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.share_mode(1).custom_flags(0x0200_0000); // READ sharing and backup semantics.
}

#[cfg(not(any(unix, windows)))]
pub(super) fn directory_options(_: &mut OpenOptions) {}

#[cfg(unix)]
pub(super) fn file_options(options: &mut OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.custom_flags(libc::O_NONBLOCK);
}

#[cfg(windows)]
pub(super) fn file_options(options: &mut OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    const GENERIC_READ: u32 = 0x8000_0000;
    const READ_CONTROL: u32 = 0x0002_0000;
    options.access_mode(GENERIC_READ | READ_CONTROL).share_mode(1);
}

#[cfg(not(any(unix, windows)))]
pub(super) fn file_options(_: &mut OpenOptions) {}

#[cfg(unix)]
pub(super) fn final_binding(parent: &Dir, name: &OsStr, original: &Metadata) -> io::Result<()> {
    let current = parent.symlink_metadata(name)?;
    if !current.is_file() || linked(&current) || identity(&current)? != identity(original)? {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn final_binding(_: &Dir, _: &OsStr, original: &Metadata) -> io::Result<()> {
    // The original file and every child directory retain no-delete sharing.
    // This binds each pathname without reopening the original file.
    if !original.is_file() || linked(original) {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub(super) fn final_binding(_: &Dir, _: &OsStr, _: &Metadata) -> io::Result<()> {
    Err(invalid())
}

#[cfg(unix)]
pub(super) fn same_state(left: &Metadata, right: &Metadata) -> bool {
    use cap_std::fs::MetadataExt as _;
    // Access time is intentionally excluded: these same-handle reads may update it.
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.mode() == right.mode()
        && left.uid() == right.uid()
        && left.gid() == right.gid()
        && left.nlink() == right.nlink()
        && left.rdev() == right.rdev()
        && left.size() == right.size()
        && left.blocks() == right.blocks()
        && left.blksize() == right.blksize()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

#[cfg(windows)]
pub(super) fn same_state(left: &Metadata, right: &Metadata) -> bool {
    use cap_primitives::fs::_WindowsByHandle as ByHandle;
    use cap_std::fs::MetadataExt as OsMetadata;
    ByHandle::volume_serial_number(left) == ByHandle::volume_serial_number(right)
        && ByHandle::file_index(left) == ByHandle::file_index(right)
        && ByHandle::number_of_links(left) == ByHandle::number_of_links(right)
        && OsMetadata::file_attributes(left) == OsMetadata::file_attributes(right)
        && left.file_size() == right.file_size()
        && left.creation_time() == right.creation_time()
        && left.last_write_time() == right.last_write_time()
}

#[cfg(not(any(unix, windows)))]
pub(super) fn same_state(_: &Metadata, _: &Metadata) -> bool {
    false
}

pub(super) fn invalid() -> io::Error {
    io::Error::other("private command input could not be authenticated")
}
