use crate::diagnostics::architecture_error;
use crate::filesystem::identity::open_controlled_file;
use crate::filesystem::identity::revalidate_controlled_file;
use crate::filesystem::identity::validate_current_controlled_path;
use std::io::Read;
use std::path::Path;
use zryna_diagnostics::Diagnostic;

pub(crate) fn read_bounded_utf8(
    path: &Path,
    diagnostic_path: Option<&Path>,
    max_bytes: u64,
    unavailable_code: &str,
    unavailable_guidance: &str,
) -> Result<(String, u64), Diagnostic> {
    let policy = ControlledReadPolicy {
        diagnostic_path,
        max_bytes,
        unavailable_code,
        unavailable_guidance,
        expected_size: None,
    };
    read_bounded_utf8_with_hooks(path, policy, || {}, || {})
}

pub(crate) fn read_bounded_utf8_with_expected_size(
    path: &Path,
    diagnostic_path: Option<&Path>,
    max_bytes: u64,
    unavailable_code: &str,
    unavailable_guidance: &str,
    expected_size: u64,
) -> Result<(String, u64), Diagnostic> {
    let policy = ControlledReadPolicy {
        diagnostic_path,
        max_bytes,
        unavailable_code,
        unavailable_guidance,
        expected_size: Some(expected_size),
    };
    read_bounded_utf8_with_hooks(path, policy, || {}, || {})
}

#[derive(Clone, Copy)]
pub(crate) struct ControlledReadPolicy<'a> {
    pub(crate) diagnostic_path: Option<&'a Path>,
    pub(crate) max_bytes: u64,
    pub(crate) unavailable_code: &'a str,
    pub(crate) unavailable_guidance: &'a str,
    pub(crate) expected_size: Option<u64>,
}

pub(crate) fn read_bounded_utf8_with_hooks<BeforeOpen, AfterRead>(
    path: &Path,
    policy: ControlledReadPolicy<'_>,
    before_open: BeforeOpen,
    after_read: AfterRead,
) -> Result<(String, u64), Diagnostic>
where
    BeforeOpen: FnOnce(),
    AfterRead: FnOnce(),
{
    let (mut handle, opened) = open_controlled_file(
        path,
        policy.diagnostic_path,
        policy.max_bytes,
        policy.unavailable_code,
        policy.unavailable_guidance,
    )?;
    if policy.expected_size.is_some_and(|expected| expected != opened.len()) {
        return Err(architecture_error(
            "ZRYNA-A1203",
            policy.diagnostic_path,
            "controlled file size changed before its bounded read",
            "stop concurrent replacement and retry architecture validation",
        ));
    }
    before_open();
    validate_current_controlled_path(path, policy.diagnostic_path, &handle, &opened)?;

    let mut bytes = Vec::new();
    handle.as_file_mut().take(policy.max_bytes.saturating_add(1)).read_to_end(&mut bytes).map_err(
        |error| {
            architecture_error(
                "ZRYNA-A1203",
                policy.diagnostic_path,
                format!("controlled file could not be read completely: {error}"),
                "restore stable read access and retry",
            )
        },
    )?;
    if u64::try_from(bytes.len()).map_or(true, |length| length > policy.max_bytes) {
        return Err(architecture_error(
            "ZRYNA-A1204",
            policy.diagnostic_path,
            format!("controlled file exceeds its {}-byte safety limit", policy.max_bytes),
            "reduce the file before architecture validation",
        ));
    }

    after_read();
    revalidate_controlled_file(path, policy.diagnostic_path, &handle, &opened)?;
    let length = u64::try_from(bytes.len()).map_err(|_| {
        architecture_error(
            "ZRYNA-A1204",
            policy.diagnostic_path,
            "controlled file length cannot be represented safely",
            "reduce the file before architecture validation",
        )
    })?;
    String::from_utf8(bytes).map(|source| (source, length)).map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            policy.diagnostic_path,
            format!("controlled file is not valid UTF-8: {error}"),
            "save controlled source and manifest files as UTF-8",
        )
    })
}
