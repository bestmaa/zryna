//! Checked UTF-16 byte lengths and ABI-aligned owned rename allocation.

use super::{
    AsRawHandle, BorrowedHandle, FILE_RENAME_INFORMATION, HANDLE, align_of, fmt, io, size_of,
};

pub(super) struct RenameBuffer {
    words: Vec<usize>,
    pub(super) byte_len: u32,
    pub(super) name_bytes: usize,
}

pub(super) fn allocate(name: &[u16]) -> io::Result<RenameBuffer> {
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
    let buffer = RenameBuffer {
        words: vec![0; word_count],
        name_bytes,
        byte_len: u32::try_from(byte_len).map_err(invalid_input)?,
    };
    Ok(buffer)
}

impl RenameBuffer {
    pub(super) fn as_mut_ptr(&mut self) -> *mut FILE_RENAME_INFORMATION {
        self.words.as_mut_ptr().cast()
    }
}

pub(super) fn utf16_byte_length_u16(units: usize) -> io::Result<u16> {
    units
        .checked_mul(size_of::<u16>())
        .and_then(|bytes| u16::try_from(bytes).ok())
        .ok_or_else(|| invalid_input("directory component byte length overflow"))
}

pub(super) fn invalid_input(error: impl fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error.to_string())
}

pub(super) fn raw_handle(handle: BorrowedHandle<'_>) -> HANDLE {
    handle.as_raw_handle().cast()
}

pub(super) fn token_sid_start(base: usize, sid: usize) -> io::Result<usize> {
    let sid_start = sid.checked_sub(base).ok_or_else(crate::private_grant_policy::rejected)?;
    if sid_start < size_of::<windows_sys::Win32::Security::TOKEN_USER>() || sid_start % 4 != 0 {
        return Err(crate::private_grant_policy::rejected());
    }
    Ok(sid_start)
}
