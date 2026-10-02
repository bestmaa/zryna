//! Actual Windows handle and ACL rejection evidence for the explicit grant-file boundary.

#![cfg(windows)]

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
};
use zryna_windows_filesystem::admit_private_grant_file;

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);
const READ_CONTROL: u32 = 0x0002_0000;
const GENERIC_READ: u32 = 0x8000_0000;

fn open(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(GENERIC_READ | READ_CONTROL)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[test]
fn actual_private_acl_retains_one_handle_and_broad_acl_inputs_reject()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Fixture::new()?;
    let proof = admit_private_grant_file(open(&root.path.join("private.json"))?)?;
    let mut captured = String::new();
    proof.file().read_to_string(&mut captured)?;
    assert_eq!(captured, "immutable grant input");
    proof.revalidate()?;
    for name in
        ["everyone.json", "administrators.json", "inherited.json", "null.json", "callback.json"]
    {
        assert!(admit_private_grant_file(open(&root.path.join(name))?).is_err(), "{name}");
    }
    assert!(
        OpenOptions::new().write(true).open(root.path.join("private.json")).is_err(),
        "the retained original handle excludes writers"
    );
    assert!(
        fs::rename(root.path.join("private.json"), root.path.join("replaced.json")).is_err(),
        "the retained original handle excludes pathname replacement"
    );
    drop(proof);
    open(&root.path.join("private.json"))?;
    Ok(())
}

#[test]
fn single_link_and_original_handle_security_access_are_required()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Fixture::new()?;
    let path = root.path.join("private.json");
    let restricted = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&path)?;
    assert!(admit_private_grant_file(restricted).is_err(), "READ_CONTROL was not acquired");
    let alias = root.path.join("alias.json");
    fs::hard_link(&path, &alias)?;
    assert!(admit_private_grant_file(open(&path)?).is_err(), "additional links reject");
    fs::remove_file(alias)?;
    admit_private_grant_file(open(&path)?)?.revalidate()?;
    Ok(())
}

#[test]
fn changed_dacl_is_detected_on_the_retained_original_handle()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Fixture::new()?;
    let path = root.path.join("private.json");
    let proof = admit_private_grant_file(open(&path)?)?;
    let script = r"
$ErrorActionPreference = 'Stop'
$path = $env:ZRYNA_PRIVATE_GRANT_FIXTURE
$acl = [System.IO.File]::GetAccessControl($path)
$everyone = [System.Security.Principal.SecurityIdentifier]::new('S-1-1-0')
$acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($everyone,'Read','Allow'))
[System.IO.File]::SetAccessControl($path, $acl)
";
    let output = Command::new("powershell.exe")
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script])
        .env("ZRYNA_PRIVATE_GRANT_FIXTURE", &path)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(String::from_utf8_lossy(&output.stderr)).into());
    }
    assert!(proof.revalidate().is_err(), "same-handle post-capture policy changed");
    Ok(())
}

struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!(
            "zryna-private-grant-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        let fixture = Self { path };
        // The fixed script mutates only newly created test fixtures. Paths enter as one native
        // environment value, never through command text or shell interpolation.
        let script = r"
$ErrorActionPreference = 'Stop'
$root = $env:ZRYNA_PRIVATE_GRANT_FIXTURE
$user = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$system = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18')
$everyone = [System.Security.Principal.SecurityIdentifier]::new('S-1-1-0')
$administrators = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-32-544')
foreach ($name in @('private.json','everyone.json','administrators.json','inherited.json','null.json','callback.json')) {
    $path = Join-Path $root $name
    [System.IO.File]::WriteAllText($path, 'immutable grant input')
    $acl = [System.Security.AccessControl.FileSecurity]::new()
    $acl.SetOwner($user)
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($sid in @($user,$system)) {
        $rule = [System.Security.AccessControl.FileSystemAccessRule]::new($sid, 'FullControl', 'Allow')
        $acl.AddAccessRule($rule)
    }
    if ($name -eq 'everyone.json') {
        $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($everyone,'Read','Allow'))
    }
    if ($name -eq 'administrators.json') {
        $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($administrators,'Read','Allow'))
    }
    [System.IO.File]::SetAccessControl($path, $acl)
}
$nullPath = Join-Path $root 'null.json'
$nullAcl = [System.IO.File]::GetAccessControl($nullPath)
$nullAcl.SetSecurityDescriptorSddlForm('D:NO_ACCESS_CONTROL', [System.Security.AccessControl.AccessControlSections]::Access)
[System.IO.File]::SetAccessControl($nullPath, $nullAcl)
$callbackPath = Join-Path $root 'callback.json'
$callbackAcl = [System.Security.AccessControl.RawAcl]::new(2, 1)
$callbackAce = [System.Security.AccessControl.CommonAce]::new('None','AccessAllowed',1,$user,$true,$null)
$callbackAcl.InsertAce(0, $callbackAce)
$raw = [System.Security.AccessControl.RawSecurityDescriptor]::new('DiscretionaryAclPresent, SelfRelative',$user,$null,$null,$callbackAcl)
$binary = [byte[]]::new($raw.BinaryLength)
$raw.GetBinaryForm($binary, 0)
$callback = [System.IO.File]::GetAccessControl($callbackPath)
$callback.SetSecurityDescriptorBinaryForm($binary, [System.Security.AccessControl.AccessControlSections]::Access)
[System.IO.File]::SetAccessControl($callbackPath, $callback)
$parent = [System.Security.AccessControl.DirectorySecurity]::new()
$parent.SetOwner($user)
$parent.SetAccessRuleProtection($true, $false)
foreach ($sid in @($user,$system,$everyone)) {
    $rights = if ($sid -eq $everyone) { 'Read' } else { 'FullControl' }
    $rule = [System.Security.AccessControl.FileSystemAccessRule]::new($sid,$rights,'ContainerInherit, ObjectInherit','None','Allow')
    $parent.AddAccessRule($rule)
}
[System.IO.Directory]::SetAccessControl($root, $parent)
$childPath = Join-Path $root 'inherited.json'
$child = [System.IO.File]::GetAccessControl($childPath)
$child.SetAccessRuleProtection($false, $false)
[System.IO.File]::SetAccessControl($childPath, $child)
";
        let output = Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script])
            .env("ZRYNA_PRIVATE_GRANT_FIXTURE", &fixture.path)
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other(String::from_utf8_lossy(&output.stderr)).into());
        }
        Ok(fixture)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for name in [
            "private.json",
            "everyone.json",
            "administrators.json",
            "inherited.json",
            "null.json",
            "callback.json",
            "alias.json",
            "replaced.json",
        ] {
            let _ = fs::remove_file(self.path.join(name));
        }
        let _ = fs::remove_dir(&self.path);
    }
}
