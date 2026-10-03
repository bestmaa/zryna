//! Retained source-state checks that never reopen or reread source contents.

use std::{fs, io};

use cap_std::fs::{Dir, Metadata};
use same_file::Handle;

pub(crate) fn validate(
    directory: &Dir,
    name: &str,
    handle: &Handle,
    expected: &fs::Metadata,
) -> io::Result<()> {
    let held = handle.as_file().metadata()?;
    let current = directory.symlink_metadata(name)?;
    let held = Metadata::from_just_metadata(held);
    let expected = Metadata::from_just_metadata(expected.clone());
    if !regular(&held)
        || !regular(&current)
        || !same_state(&expected, &held)
        || !same_state(&expected, &current)
    {
        return Err(io::Error::other("retained source identity or state changed"));
    }
    Ok(())
}

#[cfg(unix)]
fn regular(metadata: &Metadata) -> bool {
    use cap_std::fs::MetadataExt as _;
    metadata.is_file() && !metadata.is_symlink() && metadata.nlink() == 1
}

#[cfg(windows)]
fn regular(metadata: &Metadata) -> bool {
    use cap_std::fs::MetadataExt as _;
    metadata.is_file() && !metadata.is_symlink() && metadata.file_attributes() & 0x400 == 0
}

#[cfg(unix)]
fn same_state(left: &Metadata, right: &Metadata) -> bool {
    use cap_std::fs::MetadataExt as _;
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.nlink() == right.nlink()
        && left.size() == right.size()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

#[cfg(windows)]
fn same_state(left: &Metadata, right: &Metadata) -> bool {
    use cap_std::fs::MetadataExt as _;
    // Final source opens deny write/delete sharing for the complete retained lifetime.
    // Handle-relative metadata observes the binding without another content open.
    left.file_attributes() == right.file_attributes()
        && left.creation_time() == right.creation_time()
        && left.last_write_time() == right.last_write_time()
        && left.file_size() == right.file_size()
}
