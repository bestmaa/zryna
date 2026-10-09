use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(in crate::command_h1_runtime) struct PrivateInput {
    root: PathBuf,
    path: PathBuf,
}

impl PrivateInput {
    pub(in crate::command_h1_runtime) fn new(key: &str, value: Option<&str>) -> io::Result<Self> {
        let root = fs::canonicalize(std::env::temp_dir())?.join(format!(
            "zryna-command-runtime-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root)?;
        let path = root.join("request.json");
        let fixture = Self { root, path };
        let mut input = serde_json::json!({"present": value.is_some()});
        if let Some(value) = value {
            input["value"] = value.into();
        }
        let request = serde_json::json!({
            "schema": "zryna.wasi-command-request.v1",
            "world": "zryna:capability-profiles/command@0.1.0",
            "grant": {"capability": "environment", "key": key}, "input": input,
        });
        fs::write(
            &fixture.path,
            serde_json::to_vec(&request)
                .map_err(|_| io::Error::other("private fixture encoding failed"))?,
        )?;
        fixture.privacy(false)?;
        Ok(fixture)
    }

    pub(in crate::command_h1_runtime) fn path(&self) -> &Path {
        &self.path
    }

    #[cfg(unix)]
    pub(in crate::command_h1_runtime) fn privacy(&self, broad: bool) -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(
            &self.path,
            fs::Permissions::from_mode(if broad { 0o644 } else { 0o600 }),
        )
    }

    #[cfg(windows)]
    pub(in crate::command_h1_runtime) fn privacy(&self, broad: bool) -> io::Result<()> {
        // Fixed test code changes only this newly created fixture's owner and DACL.
        // The native path is an environment value, never interpolated shell code.
        let script = r"
$ErrorActionPreference = 'Stop'
$path = $env:ZRYNA_COMMAND_RUNTIME_FIXTURE
$user = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$system = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18')
$acl = [System.Security.AccessControl.FileSecurity]::new()
$acl.SetOwner($user)
$acl.SetAccessRuleProtection($true, $false)
foreach ($sid in @($user,$system)) {
    $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','Allow'))
}
if ($env:ZRYNA_COMMAND_RUNTIME_BROAD -eq 'yes') {
    $everyone = [System.Security.Principal.SecurityIdentifier]::new('S-1-1-0')
    $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($everyone,'Read','Allow'))
}
[System.IO.File]::SetAccessControl($path,$acl)
";
        let output = crate::process_spawn::output(
            std::process::Command::new("powershell.exe")
                .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script])
                .env("ZRYNA_COMMAND_RUNTIME_FIXTURE", &self.path)
                .env("ZRYNA_COMMAND_RUNTIME_BROAD", if broad { "yes" } else { "no" }),
        )?;
        if !output.status.success() {
            return Err(io::Error::other("fixed private runtime fixture ACL setup failed"));
        }
        Ok(())
    }

    #[cfg(not(any(windows, unix)))]
    pub(in crate::command_h1_runtime) fn privacy(&self, _: bool) -> io::Result<()> {
        Err(io::Error::other("private runtime fixture unsupported on this platform"))
    }

    #[cfg(windows)]
    pub(in crate::command_h1_runtime) fn assert_retained(&self) {
        assert!(fs::OpenOptions::new().write(true).open(&self.path).is_err());
        assert!(fs::rename(&self.path, self.root.join("replacement.json")).is_err());
    }

    pub(in crate::command_h1_runtime) fn assert_released(&self) -> io::Result<()> {
        let moved = self.root.join("released.json");
        fs::rename(&self.path, &moved)?;
        fs::rename(moved, &self.path)
    }
}

impl Drop for PrivateInput {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_dir(&self.root);
    }
}
