use super::{MAX_COMPONENT_UNITS, invalid_input};
use std::{ffi::OsStr, io, os::windows::ffi::OsStrExt};
pub(super) fn encode_component(name: &OsStr) -> io::Result<Vec<u16>> {
    let units: Vec<u16> = name.encode_wide().take(MAX_COMPONENT_UNITS + 1).collect();
    if units.is_empty() || units.len() > MAX_COMPONENT_UNITS {
        return Err(invalid_input("directory name must contain 1 to 255 UTF-16 units"));
    }
    let bytes: Vec<u8> = units
        .iter()
        .copied()
        .map(u8::try_from)
        .collect::<Result<_, _>>()
        .map_err(|_| invalid_input("directory name must be portable ASCII"))?;
    if bytes == b"." || bytes == b".." {
        return Err(invalid_input("dot directory components are not permitted"));
    }
    if bytes
        .iter()
        .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
        || bytes.last() == Some(&b'.')
    {
        return Err(invalid_input("directory name is not a portable ASCII component"));
    }
    let stem = bytes.split(|byte| *byte == b'.').next().unwrap_or(&bytes);
    if is_reserved_device_stem(stem) {
        return Err(invalid_input("reserved Windows device name is not permitted"));
    }
    Ok(units)
}

fn is_reserved_device_stem(stem: &[u8]) -> bool {
    [b"CON".as_slice(), b"PRN", b"AUX", b"NUL"]
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
        || ((stem.len() == 4)
            && (stem[..3].eq_ignore_ascii_case(b"COM") || stem[..3].eq_ignore_ascii_case(b"LPT"))
            && stem[3].is_ascii_digit())
}
