use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn guard() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub struct Case {
    pub root: PathBuf,
    pub stem: String,
    pub bundle: PathBuf,
}

impl Case {
    pub fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace");
        let stem =
            format!("wasi-command-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let bundle = root.join(".zryna/out").join(format!("{stem}.wasi-command-run"));
        Self { root, stem, bundle }
    }

    pub fn run(&self, source: &str, input: Option<&Input>, extra: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_zryna"));
        command
            .args([
                "run",
                source,
                "--target",
                "wasi-command",
                "--profile",
                "command-h1-v1",
                "--export",
                "main",
                "--name",
                &self.stem,
                "--json",
                "--node",
            ])
            .arg(node())
            .arg("--root")
            .arg(&self.root)
            .args(extra);
        if let Some(input) = input {
            command.arg("--grant-file").arg(&input.path);
        }
        command.output().expect("actual CLI invocation")
    }

    pub fn manifest(&self) -> Vec<u8> {
        fs::read(self.bundle.join(zryna_driver::COMMAND_H1_MANIFEST_NAME))
            .expect("complete manifest")
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        // This path is the exact unique bundle this fixture created below its retained workspace.
        if self.bundle.parent() == Some(self.root.join(".zryna/out").as_path()) {
            let _ = fs::remove_dir_all(&self.bundle);
        }
    }
}

pub struct Input {
    root: PathBuf,
    pub path: PathBuf,
}

impl Input {
    pub fn new(key: &str, value: Option<&str>) -> Self {
        let root = fs::canonicalize(env::temp_dir()).expect("temp root").join(format!(
            "wasi-command-input-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("unique private fixture directory");
        let path = root.join("request.json");
        let mut input = serde_json::json!({"present": value.is_some()});
        if let Some(value) = value {
            input["value"] = value.into();
        }
        let request = serde_json::json!({"schema": "zryna.wasi-command-request.v1",
            "world": "zryna:capability-profiles/command@0.1.0",
            "grant": {"capability": "environment", "key": key}, "input": input});
        let bytes = serde_json::to_vec(&request).expect("request JSON");
        let result = Self { root, path };
        result.write_private(&bytes);
        result
    }

    #[cfg(unix)]
    fn write_private(&self, bytes: &[u8]) {
        use std::{io::Write as _, os::unix::fs::OpenOptionsExt as _};
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&self.path)
            .expect("create owner-private input");
        file.write_all(bytes).expect("write private input");
    }

    #[cfg(windows)]
    fn write_private(&self, bytes: &[u8]) {
        let script = r"
$ErrorActionPreference = 'Stop'
$path = $env:ZRYNA_COMMAND_CLI_PRIVATE_FILE
$bytes = [Text.Encoding]::UTF8.GetBytes($env:ZRYNA_COMMAND_CLI_PRIVATE_JSON)
$user = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$system = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18')
$acl = [System.Security.AccessControl.FileSecurity]::new()
$acl.SetOwner($user)
$acl.SetAccessRuleProtection($true, $false)
foreach ($sid in @($user,$system)) {
  $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','Allow'))
}
$file = [IO.FileStream]::new($path, [IO.FileMode]::CreateNew, [Security.AccessControl.FileSystemRights]::Write, [IO.FileShare]::None, 4096, [IO.FileOptions]::None, $acl)
try { $file.Write($bytes, 0, $bytes.Length) } finally { $file.Dispose() }
";
        let output = Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script])
            .env("ZRYNA_COMMAND_CLI_PRIVATE_FILE", &self.path)
            .env("ZRYNA_COMMAND_CLI_PRIVATE_JSON", std::str::from_utf8(bytes).expect("JSON UTF8"))
            .output()
            .expect("fixed ACL fixture helper");
        assert!(output.status.success(), "fixture private ACL setup");
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_dir(&self.root);
    }
}

fn node() -> PathBuf {
    let name = if cfg!(windows) { "node.exe" } else { "node" };
    let configured =
        ["ZRYNA_TEST_NODE", "NODE"].into_iter().filter_map(env::var_os).map(PathBuf::from);
    let paths = env::var_os("PATH")
        .map(|path| env::split_paths(&path).map(|dir| dir.join(name)).collect::<Vec<_>>())
        .unwrap_or_default();
    let node = configured
        .chain(paths)
        .find(|path| path.is_file())
        .expect("pinned Node")
        .canonicalize()
        .expect("Node path");
    let version = Command::new(&node).arg("--version").output().expect("Node probe");
    assert!(version.status.success());
    assert!(matches!(version.stdout.as_slice(), b"v22.22.1\n" | b"v22.22.1\r\n"));
    node
}

pub fn read_json(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).expect("closed JSON")
}
pub fn input_path(input: &Input) -> &Path {
    &input.path
}
