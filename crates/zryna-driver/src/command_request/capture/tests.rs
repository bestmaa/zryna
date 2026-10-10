#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::CapturedRequest;

static NEXT: AtomicU64 = AtomicU64::new(0);
const VALID: &str = r#"{"schema":"zryna.wasi-command-request.v1","world":"zryna:capability-profiles/command@0.1.0","grant":{"capability":"environment","key":"MODE"},"input":{"present":true,"value":"on"}}"#;

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> io::Result<Self> {
        let root = fs::canonicalize(std::env::temp_dir())?.join(format!(
            "zryna-command-input-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root)?;
        fs::create_dir(root.join("nested"))?;
        fs::write(root.join("nested/request.json"), VALID)?;
        Ok(Self { root })
    }

    fn path(&self) -> PathBuf {
        self.root.join("nested/request.json")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for name in ["alias.json", "original.json", "alias-dir"] {
            let _ = fs::remove_file(self.root.join(name));
        }
        let _ = fs::remove_dir(self.root.join("junction"));
        for name in ["nested", "original-dir"] {
            let directory = self.root.join(name);
            let _ = fs::remove_file(directory.join("request.json"));
            let _ = fs::remove_dir(directory);
        }
        let _ = fs::remove_dir(&self.root);
    }
}

#[track_caller]
fn rejected(path: &Path, key: Option<&str>) {
    let diagnostic = CapturedRequest::capture(path, key).err().expect("rejected capture");
    assert_eq!(diagnostic, super::super::rejection());
}

// Write to the actual stream so successful tests expose their fixture path even
// when the test harness captures printing macros.
fn owner_fixture_evidence(message: std::fmt::Arguments<'_>) -> io::Result<()> {
    use std::io::Write as _;

    writeln!(io::stderr().lock(), "{message}")
}
