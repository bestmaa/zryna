use super::*;

pub(super) const FIXTURE_LIMITS: ScanLimits =
    ScanLimits { entries: 128, depth: 16, file_bytes: 128, total_bytes: 1024, diagnostics: 8 };

pub(super) static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

pub(super) struct TempFixture {
    pub(super) root: PathBuf,
}

impl TempFixture {
    pub(super) fn new() -> std::io::Result<Self> {
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("zryna-architecture-{}-{sequence}", std::process::id()));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub(super) fn path(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.root.join(relative)
    }

    pub(super) fn directory(&self, relative: impl AsRef<Path>) -> std::io::Result<PathBuf> {
        let path = self.path(relative);
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    pub(super) fn write(
        &self,
        relative: impl AsRef<Path>,
        bytes: &[u8],
    ) -> std::io::Result<PathBuf> {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, bytes)?;
        Ok(path)
    }
}

impl Drop for TempFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(super) fn fixture_contract() -> WorkspaceContract {
    WorkspaceContract {
        schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
        version: CONTRACT_VERSION,
        profile: CONTRACT_PROFILE.to_owned(),
        members: Vec::new(),
        adapters: vec![AdapterContract {
            id: "typescript-6".to_owned(),
            root: "adapters/typescript-6".to_owned(),
            protocol_version: 1,
            toolchain: "@typescript/typescript6@6.0.2".to_owned(),
            allowed_entries: Vec::new(),
        }],
        outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
    }
}

pub(super) fn write_minimal_workspace(fixture: &TempFixture) -> Result<(), Box<dyn Error>> {
    let contract = WorkspaceContract {
        schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
        version: CONTRACT_VERSION,
        profile: CONTRACT_PROFILE.to_owned(),
        members: vec![MemberContract {
            id: "sample".to_owned(),
            root: "crates/sample".to_owned(),
            kind: MemberKind::Foundation,
            dependencies: Vec::new(),
            allowed_entries: vec![
                "Cargo.toml".to_owned(),
                "README.md".to_owned(),
                "src".to_owned(),
            ],
        }],
        adapters: Vec::new(),
        outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
    };
    fixture.write("zryna.workspace.json", &serde_json::to_vec_pretty(&contract)?)?;
    fixture
        .write("Cargo.toml", b"[workspace]\nresolver = \"2\"\nmembers = [\"crates/sample\"]\n")?;
    fixture.write(
        "crates/sample/Cargo.toml",
        b"[package]\nname = \"sample\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fixture.write("crates/sample/README.md", b"# sample\n")?;
    fixture.write("crates/sample/src/lib.rs", b"//! Sample fixture.\n")?;
    Ok(())
}

pub(super) fn scan_fixture(
    root: &Path,
    limits: ScanLimits,
) -> (bool, Vec<zryna_diagnostics::Diagnostic>) {
    let contract = fixture_contract();
    let policy = ScanPolicy::new(&contract, "");
    let mut diagnostics = ValidationDiagnostics::default();
    let mut state = ScanState::new(limits);
    scan_path(root, root, &policy, 0, &mut state, &mut diagnostics);
    (!state.halted, diagnostics.into_vec())
}

pub(super) fn has_code(diagnostics: &[zryna_diagnostics::Diagnostic], code: &str) -> bool {
    diagnostics.iter().any(|diagnostic| diagnostic.code == code)
}

#[cfg(unix)]
pub(super) fn successful_exit_status() -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt;

    std::process::ExitStatus::from_raw(0)
}

#[cfg(windows)]
pub(super) fn successful_exit_status() -> std::process::ExitStatus {
    use std::os::windows::process::ExitStatusExt;

    std::process::ExitStatus::from_raw(0)
}
