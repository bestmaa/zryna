use std::{fs, io, os::windows::fs::MetadataExt as _, path::Path, process::Command};

use super::{CapturedRequest, Fixture, VALID, owner_fixture_evidence, rejected};

fn acl(path: &Path, broad: bool) -> io::Result<()> {
    // Only fixed test code executes; the native path is passed as one environment value.
    let script = r"
$ErrorActionPreference = 'Stop'
$path = $env:ZRYNA_COMMAND_INPUT_FIXTURE
$user = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$system = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18')
$acl = [System.Security.AccessControl.FileSecurity]::new()
$acl.SetOwner($user)
$acl.SetAccessRuleProtection($true, $false)
foreach ($sid in @($user,$system)) {
    $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','Allow'))
}
if ($env:ZRYNA_COMMAND_INPUT_BROAD -eq 'yes') {
    $everyone = [System.Security.Principal.SecurityIdentifier]::new('S-1-1-0')
    $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($everyone,'Read','Allow'))
}
[System.IO.File]::SetAccessControl($path,$acl)
";
    let output = crate::process_spawn::output(
        Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script])
            .env("ZRYNA_COMMAND_INPUT_FIXTURE", path)
            .env("ZRYNA_COMMAND_INPUT_BROAD", if broad { "yes" } else { "no" }),
    )?;
    if !output.status.success() {
        return Err(io::Error::other("fixed private fixture ACL setup failed"));
    }
    Ok(())
}

fn private() -> io::Result<Fixture> {
    let fixture = Fixture::new()?;
    acl(&fixture.path(), false)?;
    Ok(fixture)
}

#[test]
fn actual_same_handle_capture_locks_final_file_and_ancestors() -> io::Result<()> {
    let fixture = private()?;
    let path = fixture.path();
    let captured = CapturedRequest::capture(&path, Some("MODE")).expect("actual private capture");
    for _ in 0..3 {
        captured.revalidate().expect("same handle stable");
        assert!(matches!(captured.request().value(), Some("on")));
    }
    assert!(fs::OpenOptions::new().write(true).open(&path).is_err());
    assert!(fs::rename(&path, fixture.root.join("original.json")).is_err());
    assert!(fs::rename(fixture.root.join("nested"), fixture.root.join("original-dir")).is_err());
    drop(captured);
    fs::rename(&path, fixture.root.join("original.json"))?;
    fs::rename(fixture.root.join("original.json"), &path)?;
    CapturedRequest::capture(&path, Some("MODE")).expect("closed handle recovery");
    Ok(())
}

#[test]
fn actual_broad_acl_and_changed_same_handle_privacy_reject() -> io::Result<()> {
    let fixture = private()?;
    let path = fixture.path();
    acl(&path, true)?;
    rejected(&path, Some("MODE"));
    acl(&path, false)?;
    let captured = CapturedRequest::capture(&path, Some("MODE")).expect("private ACL recovery");
    acl(&path, true)?;
    assert!(captured.revalidate().is_err());
    assert!(matches!(captured.request().value(), Some("on")));
    drop(captured);
    acl(&path, false)?;
    CapturedRequest::capture(&path, Some("MODE")).expect("fresh private ACL capture");
    Ok(())
}

#[test]
fn actual_multiple_links_and_file_byte_bound_reject_and_recover() -> io::Result<()> {
    let fixture = private()?;
    let path = fixture.path();
    let alias = fixture.root.join("alias.json");
    fs::hard_link(&path, &alias)?;
    rejected(&path, Some("MODE"));
    fs::remove_file(alias)?;
    let mut bytes = VALID.as_bytes().to_vec();
    bytes.resize(4096, b' ');
    fs::write(&path, &bytes)?;
    let exact = CapturedRequest::capture(&path, Some("MODE")).expect("4096 bytes");
    exact.revalidate().expect("exact limit same handle");
    drop(exact);
    bytes.push(b' ');
    fs::write(&path, bytes)?;
    rejected(&path, Some("MODE"));
    fs::write(&path, VALID)?;
    CapturedRequest::capture(&path, Some("MODE")).expect("byte bound recovery");
    Ok(())
}

#[test]
fn relative_unc_parent_paths_and_source_mismatch_reject() -> io::Result<()> {
    let fixture = private()?;
    rejected(Path::new("request.json"), Some("MODE"));
    rejected(Path::new(r"\\unreachable\share\request.json"), Some("MODE"));
    // PathBuf::join normalizes parent components for a verbatim Windows prefix.
    // Preserve the raw spelling so the capture actually receives the rejected token.
    let mut parent_path = fixture.root.as_os_str().to_os_string();
    parent_path.push(r"\nested\..\nested\request.json");
    let parent_path = std::path::PathBuf::from(parent_path);
    assert!(parent_path.components().any(|part| part == std::path::Component::ParentDir));
    rejected(&parent_path, Some("MODE"));
    rejected(&fixture.root.join("nested"), Some("MODE"));
    rejected(&fixture.path(), None);
    rejected(&fixture.path(), Some("OTHER"));
    CapturedRequest::capture(&fixture.path(), Some("MODE")).expect("invalid request recovery");
    Ok(())
}

#[test]
fn actual_dacl_change_between_capture_reads_rejects_and_recovers() -> io::Result<()> {
    let fixture = private()?;
    let path = fixture.path();
    let mut changed = false;
    let result = CapturedRequest::capture_with_after_first_read(&path, Some("MODE"), || {
        acl(&path, true)?;
        changed = true;
        Ok(())
    });
    assert!(changed, "actual fixture DACL changed after the first read");
    assert!(result.is_err());
    rejected(&path, Some("MODE"));
    acl(&path, false)?;
    let fresh = CapturedRequest::capture(&path, Some("MODE")).expect("DACL capture recovery");
    fresh.revalidate().expect("recovered same-handle privacy");
    Ok(())
}

#[test]
fn actual_junction_ancestor_rejects_without_following_and_recovers() -> io::Result<()> {
    let fixture = private()?;
    let junction = fixture.root.join("junction");
    let target = fixture.root.join("nested");
    let script = r"
$ErrorActionPreference = 'Stop'
$path = $env:ZRYNA_COMMAND_INPUT_JUNCTION
$target = $env:ZRYNA_COMMAND_INPUT_TARGET
New-Item -ItemType Junction -Path $path -Target $target | Out-Null
";
    let output = crate::process_spawn::output(
        Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script])
            .env("ZRYNA_COMMAND_INPUT_JUNCTION", &junction)
            .env("ZRYNA_COMMAND_INPUT_TARGET", &target),
    )?;
    if !output.status.success() {
        return Err(io::Error::other("fixed fixture junction setup failed"));
    }
    assert!(fs::symlink_metadata(&junction)?.file_attributes() & 0x400 != 0);
    rejected(&junction.join("request.json"), Some("MODE"));
    assert!(fs::read(fixture.path())?.as_slice().eq(VALID.as_bytes()));
    fs::remove_dir(junction)?;
    let fresh = CapturedRequest::capture(&fixture.path(), Some("MODE"))
        .expect("junction rejection recovery");
    fresh.revalidate().expect("retained real ancestor");
    Ok(())
}

#[test]
fn actual_foreign_owner_rejects_when_restore_privilege_is_available() -> io::Result<()> {
    let fixture = private()?;
    let path = fixture.path();
    owner_fixture_evidence(format_args!(
        "foreign-owner fixture platform=windows stage=eligibility outcome=checking owner_sid=S-1-5-32-544"
    ))?;
    // SetOwner is confined to the new fixture. Successful setup independently verifies
    // the actual foreign owner SID; a denied setup gives no owner-rejection coverage.
    let script = r"
$ErrorActionPreference = 'Stop'
try {
$path = $env:ZRYNA_COMMAND_INPUT_FIXTURE
$foreign = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-32-544')
$acl = [System.IO.File]::GetAccessControl($path)
$acl.SetOwner($foreign)
[System.IO.File]::SetAccessControl($path,$acl)
$observed = [System.IO.File]::GetAccessControl($path).GetOwner([System.Security.Principal.SecurityIdentifier])
if ($observed -ne $foreign) { throw 'foreign fixture owner was not established' }
} catch {
  $failure = $_.Exception.GetBaseException()
  $privilege = $failure.PSObject.Properties['PrivilegeName']
  $name = if ($null -eq $privilege) { 'none-reported' } else { [string]$privilege.Value }
  [Console]::Error.WriteLine(('foreign-owner fixture platform=windows stage=setup outcome=error exception={0} hresult={1:X8} privilege={2}' -f $failure.GetType().FullName, $failure.HResult, $name))
  exit 1
}
";
    let output = crate::process_spawn::output(
        Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script])
            .env("ZRYNA_COMMAND_INPUT_FIXTURE", &path),
    )?;
    if !output.status.success() {
        for line in String::from_utf8_lossy(&output.stderr).lines() {
            if line.starts_with("foreign-owner fixture platform=windows stage=setup outcome=error ")
            {
                owner_fixture_evidence(format_args!("{line}"))?;
            }
        }
        owner_fixture_evidence(format_args!(
            "foreign-owner fixture platform=windows stage=eligibility outcome=unavailable setup_exit_code={:?}",
            output.status.code()
        ))?;
        return Ok(());
    }
    owner_fixture_evidence(format_args!(
        "foreign-owner fixture platform=windows stage=setup outcome=established owner_sid=S-1-5-32-544"
    ))?;
    rejected(&path, Some("MODE"));
    owner_fixture_evidence(format_args!(
        "foreign-owner fixture platform=windows stage=rejection outcome=observed"
    ))?;
    acl(&path, false)?;
    CapturedRequest::capture(&path, Some("MODE")).expect("foreign-owner fixture recovery");
    owner_fixture_evidence(format_args!(
        "foreign-owner fixture platform=windows stage=recovery outcome=observed"
    ))?;
    Ok(())
}
