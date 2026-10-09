//! Safe ownership and retained-parent state for exact directory capabilities.

use super::{
    AsHandle, DELETE, Dir, FILE_CREATE, FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ, OsStr,
    SYNCHRONIZE, confirm_absent, encode_component, fmt, io, mark_for_deletion, open_relative,
    rename_directory,
};

/// The exact directory created by [`create_directory`].
///
/// This capability owns the single authoritative directory handle, its retained parent, and its
/// current parent-relative name. It is the only public source accepted for rename and removal, so
/// a regular file or a path-selected replacement cannot be supplied in its place.
pub struct OwnedDirectory {
    directory: Dir,
    parent: Dir,
    name: Vec<u16>,
}

impl fmt::Debug for OwnedDirectory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("OwnedDirectory").finish_non_exhaustive()
    }
}

impl OwnedDirectory {
    /// Borrows the exact directory for capability-relative file operations.
    #[must_use]
    pub fn directory(&self) -> &Dir {
        &self.directory
    }

    /// Renames this exact directory beneath `destination_parent` without replacement.
    ///
    /// The destination parent is cloned before mutation and becomes the retained parent only after
    /// the native rename succeeds. The source is never selected by path.
    /// Every handle to a file or directory below this directory must be closed before calling this
    /// method. Windows rejects an ancestor rename while any descendant handle remains open, even
    /// when that descendant permits delete sharing. This method does not close descendant handles.
    ///
    /// # Errors
    ///
    /// Returns [`io::ErrorKind::InvalidInput`] for an invalid component, or the native error when
    /// Windows cannot clone the parent or rename the exact source handle.
    pub fn rename_noreplace(&mut self, destination_parent: &Dir, name: &OsStr) -> io::Result<()> {
        let name = encode_component(name)?;
        let retained_parent = destination_parent.try_clone()?;
        rename_directory(&self.directory, destination_parent, &name)?;
        self.parent = retained_parent;
        self.name = name;
        Ok(())
    }

    /// Removes this exact directory if it is empty, then confirms its name is absent.
    ///
    /// The directory is first marked for deletion through its authoritative handle. That handle is
    /// then closed, and the retained parent is used for a handle-relative, no-reparse open of the
    /// bound name. Success is reported only when Windows reports that name absent. A second handle
    /// that keeps deletion pending, or an object installed at the name, therefore returns an error.
    /// The consumed capability is closed on every outcome; an error after marking may mean deletion
    /// is still pending until another process closes its handle.
    ///
    /// # Errors
    ///
    /// Returns the native error if Windows cannot mark the exact empty directory for deletion, if
    /// deletion remains pending, or if absence cannot be confirmed unambiguously.
    pub fn remove_empty(self) -> io::Result<()> {
        let Self { directory, parent, name } = self;
        let source = directory.into_std_file();
        mark_for_deletion(&source)?;
        drop(source);
        confirm_absent(parent.as_handle(), &name)
    }
}

/// Atomically creates one directory relative to `parent` and returns its exact capability.
///
/// The authoritative handle has delete access and permits read/write sharing, but deliberately
/// excludes delete sharing. `name` must be one portable ASCII path component. At most 256 UTF-16
/// units are inspected and allocated while validating the 255-unit limit. A capability created
/// below another [`OwnedDirectory`] must be dropped before renaming that ancestor.
///
/// # Errors
///
/// Returns [`io::ErrorKind::InvalidInput`] for an invalid component, or the mapped native error
/// when Windows cannot clone the parent, create the directory, or grant the required rights.
pub fn create_directory(parent: &Dir, name: &OsStr) -> io::Result<OwnedDirectory> {
    let name = encode_component(name)?;
    let retained_parent = parent.try_clone()?;
    let source = open_relative(
        parent.as_handle(),
        &name,
        GENERIC_READ | DELETE | SYNCHRONIZE,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        FILE_CREATE,
    )?;
    Ok(OwnedDirectory { directory: Dir::from_std_file(source), parent: retained_parent, name })
}
