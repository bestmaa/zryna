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

#[path = "../fixtures/private_file.rs"]
mod private_file;
pub use private_file::{Input, node};

impl Input {
    pub fn new(key: &str, value: Option<&str>) -> Self {
        let mut input = serde_json::json!({"present": value.is_some()});
        if let Some(value) = value {
            input["value"] = value.into();
        }
        let request = serde_json::json!({"schema": "zryna.wasi-command-request.v1",
            "world": "zryna:capability-profiles/command@0.1.0",
            "grant": {"capability": "environment", "key": key}, "input": input});
        let bytes = serde_json::to_vec(&request).expect("request JSON");
        Self::from_bytes(&bytes)
    }
}

pub fn read_json(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).expect("closed JSON")
}
pub fn input_path(input: &Input) -> &Path {
    &input.path
}
