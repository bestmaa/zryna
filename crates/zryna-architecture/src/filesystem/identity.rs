use crate::diagnostics::architecture_error;
use same_file::Handle;
use std::fs;
use std::path::Path;
use zryna_diagnostics::Diagnostic;

pub(crate) fn open_controlled_file(
    path: &Path,
    diagnostic_path: Option<&Path>,
    max_bytes: u64,
    unavailable_code: &str,
    unavailable_guidance: &str,
) -> Result<(Handle, fs::Metadata), Diagnostic> {
    let link_metadata = fs::symlink_metadata(path).map_err(|error| {
        architecture_error(
            unavailable_code,
            diagnostic_path,
            format!("controlled file is unavailable: {error}"),
            unavailable_guidance,
        )
    })?;
    if metadata_is_link_or_reparse(&link_metadata) || !link_metadata.is_file() {
        return Err(architecture_error(
            "ZRYNA-A1201",
            diagnostic_path,
            "controlled input must be a regular, non-symlink file",
            "replace it with a regular in-workspace UTF-8 file",
        ));
    }
    let file = open_regular_no_follow(path).map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("controlled file could not be opened safely: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    let handle = Handle::from_file(file).map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("controlled file identity is unavailable: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    let opened = handle.as_file().metadata().map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("opened file metadata is unavailable: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    if metadata_is_link_or_reparse(&opened) || !opened.is_file() {
        return Err(architecture_error(
            "ZRYNA-A1201",
            diagnostic_path,
            "opened controlled input is not a regular file",
            "replace it with a regular in-workspace UTF-8 file",
        ));
    }
    if opened.len() > max_bytes {
        return Err(architecture_error(
            "ZRYNA-A1204",
            diagnostic_path,
            format!("controlled file exceeds its {max_bytes}-byte safety limit"),
            "reduce the file before architecture validation",
        ));
    }
    validate_current_controlled_path(path, diagnostic_path, &handle, &opened)?;
    Ok((handle, opened))
}

pub(crate) fn validate_current_controlled_path(
    path: &Path,
    diagnostic_path: Option<&Path>,
    reference: &Handle,
    opened: &fs::Metadata,
) -> Result<(), Diagnostic> {
    let path_metadata = fs::symlink_metadata(path).map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("controlled file path changed during inspection: {error}"),
            "stop concurrent replacement and retry architecture validation",
        )
    })?;
    if metadata_is_link_or_reparse(&path_metadata) || !path_metadata.is_file() {
        return Err(architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            "controlled file path became a link or non-file during inspection",
            "stop concurrent replacement and retry architecture validation",
        ));
    }
    let file = open_regular_no_follow(path).map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("controlled file could not be opened safely: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    let current = Handle::from_file(file).map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("controlled file identity is unavailable: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    let current_metadata = current.as_file().metadata().map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("current file metadata is unavailable: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    if reference != &current || !same_file_state(opened, &current_metadata) {
        return Err(architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            "controlled file identity or state changed during inspection",
            "stop concurrent replacement and retry architecture validation",
        ));
    }
    Ok(())
}

pub(crate) fn revalidate_controlled_file(
    path: &Path,
    diagnostic_path: Option<&Path>,
    handle: &Handle,
    opened: &fs::Metadata,
) -> Result<(), Diagnostic> {
    let after = handle.as_file().metadata().map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("controlled file could not be revalidated: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    if !same_file_state(opened, &after) {
        return Err(architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            "controlled file changed during inspection",
            "stop concurrent modification and retry architecture validation",
        ));
    }
    validate_current_controlled_path(path, diagnostic_path, handle, opened)
}

#[cfg(unix)]
fn open_regular_no_follow(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = fs::OpenOptions::new();
    options.read(true);
    options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK);
    options.open(path)
}

#[cfg(windows)]
fn open_regular_no_follow(path: &Path) -> std::io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    let mut options = fs::OpenOptions::new();
    options.read(true).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).share_mode(FILE_SHARE_READ);
    options.open(path)
}

#[cfg(not(any(unix, windows)))]
fn open_regular_no_follow(_path: &Path) -> std::io::Result<fs::File> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "no fail-closed no-follow file strategy exists for this platform",
    ))
}

#[cfg(windows)]
pub(crate) fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
pub(crate) fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

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
