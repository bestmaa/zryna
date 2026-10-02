#![allow(unsafe_code)]

use cap_std::fs::Dir;
use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::io;
use std::mem::{align_of, size_of};
mod components;
use components::encode_component;
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::ptr::{addr_of_mut, null, null_mut};
use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_CREATE, FILE_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_RENAME_INFORMATION,
    FILE_RENAME_INFORMATION_0, FILE_SYNCHRONOUS_IO_NONALERT, FileRenameInformation, NtCreateFile,
    NtSetInformationFile,
};
use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_INVALID_HANDLE, ERROR_PATH_NOT_FOUND,
    GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE, OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError,
    STATUS_SUCCESS, UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_DISPOSITION_INFO, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileDispositionInfo, SYNCHRONIZE,
    SetFileInformationByHandle,
};
use windows_sys::Win32::System::IO::{IO_STATUS_BLOCK, IO_STATUS_BLOCK_0};

const MAX_COMPONENT_UNITS: usize = 255;

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
        let mut buffer = RenameBuffer::new(raw_handle(destination_parent.as_handle()), &name)?;

        let mut status_block = IO_STATUS_BLOCK {
            Anonymous: IO_STATUS_BLOCK_0 { Status: STATUS_SUCCESS },
            Information: 0,
        };

        // SAFETY: `RenameBuffer` owns an initialized, ABI-aligned FILE_RENAME_INFORMATION byte
        // range of the reported length. The two handles remain live throughout the synchronous
        // call.
        // ReplaceIfExists is false and no source pathname or fallback is supplied.
        let status = unsafe {
            NtSetInformationFile(
                raw_handle(self.directory.as_handle()),
                &raw mut status_block,
                buffer.as_mut_ptr().cast(),
                buffer.byte_len,
                FileRenameInformation,
            )
        };
        if status < 0 {
            return Err(error_from_ntstatus(status));
        }
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

fn open_relative(
    parent: BorrowedHandle<'_>,
    name: &[u16],
    desired_access: u32,
    share_access: u32,
    disposition: u32,
) -> io::Result<File> {
    let name_bytes = utf16_byte_length_u16(name.len())?;
    let unicode_name = UNICODE_STRING {
        Length: name_bytes,
        MaximumLength: name_bytes,
        Buffer: name.as_ptr().cast_mut(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>()).map_err(invalid_input)?,
        RootDirectory: raw_handle(parent),
        ObjectName: &raw const unicode_name,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: null(),
        SecurityQualityOfService: null(),
    };
    let mut status_block =
        IO_STATUS_BLOCK { Anonymous: IO_STATUS_BLOCK_0 { Status: STATUS_SUCCESS }, Information: 0 };
    let mut handle: HANDLE = null_mut();

    // SAFETY: every pointer references a live, correctly aligned Windows ABI value for the
    // duration of the call. `parent` is borrowed, the name length is checked in bytes, and the
    // returned handle is converted to an owning Rust handle exactly once after successful return.
    let status = unsafe {
        NtCreateFile(
            &raw mut handle,
            desired_access,
            &raw const attributes,
            &raw mut status_block,
            null(),
            FILE_ATTRIBUTE_DIRECTORY,
            share_access,
            disposition,
            FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            null(),
            0,
        )
    };
    if status < 0 {
        return Err(error_from_ntstatus(status));
    }
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::from_raw_os_error(ERROR_INVALID_HANDLE.cast_signed()));
    }

    // SAFETY: successful `NtCreateFile` returned one newly owned handle, checked above for both
    // invalid sentinel values. `OwnedHandle` establishes RAII before it is transferred to File.
    let owned = unsafe { OwnedHandle::from_raw_handle(handle) };
    Ok(File::from(owned))
}

fn mark_for_deletion(source: &File) -> io::Result<()> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };

    // SAFETY: `disposition` is a live value with the exact Windows ABI type and length, and the
    // source File owns a valid handle for the duration of this synchronous call.
    let succeeded = unsafe {
        SetFileInformationByHandle(
            source.as_raw_handle().cast(),
            FileDispositionInfo,
            (&raw const disposition).cast(),
            u32::try_from(size_of::<FILE_DISPOSITION_INFO>()).map_err(invalid_input)?,
        )
    };
    if succeeded == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

fn confirm_absent(parent: BorrowedHandle<'_>, name: &[u16]) -> io::Result<()> {
    match open_relative(
        parent,
        name,
        FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        FILE_OPEN,
    ) {
        Ok(found) => {
            drop(found);
            Err(io::Error::from_raw_os_error(ERROR_ALREADY_EXISTS.cast_signed()))
        }
        Err(error)
            if matches!(error.raw_os_error(), Some(code)
                if code == ERROR_FILE_NOT_FOUND.cast_signed()
                    || code == ERROR_PATH_NOT_FOUND.cast_signed()) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

struct RenameBuffer {
    words: Vec<usize>,
    byte_len: u32,
}

impl RenameBuffer {
    fn new(destination_parent: HANDLE, name: &[u16]) -> io::Result<Self> {
        const {
            assert!(align_of::<usize>() >= align_of::<FILE_RENAME_INFORMATION>());
        }

        let name_bytes = name
            .len()
            .checked_mul(size_of::<u16>())
            .ok_or_else(|| invalid_input("directory component length overflow"))?;
        let byte_len = size_of::<FILE_RENAME_INFORMATION>()
            .checked_add(name_bytes)
            .ok_or_else(|| invalid_input("rename buffer length overflow"))?;
        let word_count = byte_len
            .checked_add(size_of::<usize>() - 1)
            .ok_or_else(|| invalid_input("rename buffer allocation overflow"))?
            / size_of::<usize>();
        let mut buffer = Self {
            words: vec![0; word_count],
            byte_len: u32::try_from(byte_len).map_err(invalid_input)?,
        };
        let info = buffer.as_mut_ptr();

        // SAFETY: the zeroed usize allocation has at least `byte_len` initialized bytes and the
        // const assertion proves sufficient FILE_RENAME_INFORMATION alignment. The reported size
        // includes the complete fixed structure plus every filename byte, as required by Windows.
        // Each field and the trailing UTF-16 byte range lie within that allocation.
        unsafe {
            (*info).Anonymous = FILE_RENAME_INFORMATION_0 { ReplaceIfExists: false };
            (*info).RootDirectory = destination_parent;
            (*info).FileNameLength = u32::try_from(name_bytes).map_err(invalid_input)?;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                addr_of_mut!((*info).FileName).cast::<u16>(),
                name.len(),
            );
        }
        Ok(buffer)
    }

    fn as_mut_ptr(&mut self) -> *mut FILE_RENAME_INFORMATION {
        self.words.as_mut_ptr().cast()
    }
}

fn raw_handle(handle: BorrowedHandle<'_>) -> HANDLE {
    handle.as_raw_handle().cast()
}

pub(crate) fn regular_file_identity(file: &File) -> io::Result<(u32, u32, u32)> {
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, GetFileInformationByHandle,
    };
    // SAFETY: this POD output is zero-initialized and remains live for the synchronous call.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the borrowed exact file handle remains live and the output has the required size.
    if unsafe { GetFileInformationByHandle(raw_handle(file.as_handle()), &raw mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
        || info.nNumberOfLinks != 1
    {
        return Err(crate::private_grant_policy::rejected());
    }
    Ok((info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow))
}

pub(crate) fn private_file_descriptor(file: &File) -> io::Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetSecurityDescriptorControl, GetSecurityDescriptorLength,
        OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SE_SELF_RELATIVE,
    };
    struct Descriptor(PSECURITY_DESCRIPTOR);
    impl Drop for Descriptor {
        fn drop(&mut self) {
            // SAFETY: GetSecurityInfo transferred this unique LocalAlloc-owned buffer on success.
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    let mut descriptor = null_mut();
    // SAFETY: the borrowed file and initialized out pointer remain live. No component is selected
    // by name, and the successful output is adopted once into the LocalFree guard below.
    let status = unsafe {
        GetSecurityInfo(
            raw_handle(file.as_handle()),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &raw mut descriptor,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status.cast_signed()));
    }
    if descriptor.is_null() {
        return Err(crate::private_grant_policy::rejected());
    }
    let descriptor = Descriptor(descriptor);
    let mut control = 0;
    let mut revision = 0;
    // SAFETY: the live system-returned descriptor is valid, and both typed outputs are initialized.
    if unsafe { GetSecurityDescriptorControl(descriptor.0, &raw mut control, &raw mut revision) }
        == 0
    {
        return Err(io::Error::last_os_error());
    }
    if control & SE_SELF_RELATIVE == 0 || revision != 1 {
        return Err(crate::private_grant_policy::rejected());
    }
    // SAFETY: GetSecurityInfo returned a valid descriptor; self-relative control was checked.
    let length = usize::try_from(unsafe { GetSecurityDescriptorLength(descriptor.0) })
        .map_err(invalid_input)?;
    if !(20..=65_536).contains(&length) {
        return Err(crate::private_grant_policy::rejected());
    }
    // SAFETY: the self-relative system buffer owns all `length` descriptor bytes, remains live
    // through the copy, and is freed only after the independent owned snapshot is complete.
    Ok(unsafe { std::slice::from_raw_parts(descriptor.0.cast::<u8>(), length) }.to_vec())
}

pub(crate) fn effective_user_sid() -> io::Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_NO_TOKEN};
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken,
    };
    let mut token = null_mut();
    // SAFETY: current-thread pseudo handle is valid; token output is initialized. The acquired
    // token is query-only and is transferred exactly once into OwnedHandle after success.
    let mut opened = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &raw mut token) };
    if opened == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_NO_TOKEN.cast_signed()) {
            return Err(error);
        }
        // SAFETY: there is no impersonation token; use the current process's query-only token.
        opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) };
    }
    if opened == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the successful token acquisition returns one valid unique owned handle.
    let token = unsafe { OwnedHandle::from_raw_handle(token.cast()) };
    let mut length = 0;
    // SAFETY: null buffer and zero length request the required size, with a live typed output.
    let queried = unsafe {
        GetTokenInformation(
            raw_handle(token.as_handle()),
            TokenUser,
            null_mut(),
            0,
            &raw mut length,
        )
    };
    if queried != 0
        || io::Error::last_os_error().raw_os_error()
            != Some(ERROR_INSUFFICIENT_BUFFER.cast_signed())
        || usize::try_from(length).map_err(invalid_input)? < size_of::<TOKEN_USER>()
        || length > 65_536
    {
        return Err(crate::private_grant_policy::rejected());
    }
    const {
        assert!(align_of::<usize>() >= align_of::<TOKEN_USER>());
    }
    let mut words =
        vec![0_usize; usize::try_from(length).map_err(invalid_input)?.div_ceil(size_of::<usize>())];
    let capacity = length;
    // SAFETY: the zeroed ABI-aligned allocation owns at least `capacity` writable bytes. The
    // token remains live, and Windows receives the exact bounded capacity, never the Vec length.
    if unsafe {
        GetTokenInformation(
            raw_handle(token.as_handle()),
            TokenUser,
            words.as_mut_ptr().cast(),
            capacity,
            &raw mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if length > capacity
        || usize::try_from(length).map_err(invalid_input)? < size_of::<TOKEN_USER>()
    {
        return Err(crate::private_grant_policy::rejected());
    }
    // SAFETY: the successful output contains a complete aligned TOKEN_USER. Its SID pointer is
    // read as an integer only; no dereference follows until its range is independently checked.
    let sid = unsafe { (*words.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let base = words.as_ptr() as usize;
    let sid_start =
        (sid as usize).checked_sub(base).ok_or_else(crate::private_grant_policy::rejected)?;
    if sid_start < size_of::<TOKEN_USER>() || sid_start % 4 != 0 {
        return Err(crate::private_grant_policy::rejected());
    }
    let length = usize::try_from(length).map_err(invalid_input)?;
    // SAFETY: the bounded, initialized allocation remains live and contains `length` bytes.
    let bytes = unsafe { std::slice::from_raw_parts(words.as_ptr().cast::<u8>(), length) };
    let sid_bytes = bytes.get(sid_start..).ok_or_else(crate::private_grant_policy::rejected)?;
    crate::private_grant_policy::sid(sid_bytes).map(<[u8]>::to_vec)
}

fn error_from_ntstatus(status: i32) -> io::Error {
    // SAFETY: RtlNtStatusToDosError accepts every NTSTATUS value and returns a Win32 error code.
    let code = unsafe { RtlNtStatusToDosError(status) };
    let code = i32::try_from(code).unwrap_or(i32::MAX);
    io::Error::from_raw_os_error(code)
}

fn utf16_byte_length_u16(units: usize) -> io::Result<u16> {
    units
        .checked_mul(size_of::<u16>())
        .and_then(|bytes| u16::try_from(bytes).ok())
        .ok_or_else(|| invalid_input("directory component byte length overflow"))
}

fn invalid_input(error: impl fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error.to_string())
}

#[cfg(test)]
mod tests;
