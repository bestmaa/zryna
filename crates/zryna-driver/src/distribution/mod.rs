//! Installation admission rooted in the independently authenticated running compiler.
//!
//! Initial archive authentication belongs to the external release verifier. A compiler cannot
//! authenticate its own initial execution by consulting a mutable adjacent manifest.

mod command_h1;
mod commands;
mod filesystem;
mod identity;
mod integrity;
mod manifest;
mod provider;
mod request;
mod source;
mod stage;
mod wire;

pub use command_h1::InstalledCommandH1Request;
pub use commands::InstalledCommandSuccess;
pub(crate) use provider::InstalledExecution;
pub(crate) use request::InstalledAdmission;
pub use request::{InstalledBuildRequest, InstalledProfile};
pub(crate) use source::InstalledProjectSources;

use std::path::{Path, PathBuf};

use same_file::Handle;
use zryna_diagnostics::Diagnostic;

use filesystem::InstallationTree;
use manifest::Distribution;

/// Retained installation proof constructed only from the running compiler's embedded identity.
///
/// There is no public constructor taking a root, receipt, expected digest, or verification flag.
pub struct InstalledCompiler {
    root: PathBuf,
    tree: InstallationTree,
    distribution: Distribution,
}

impl InstalledCompiler {
    /// Reports whether this executable was compiled with a prepared distribution identity.
    #[must_use]
    pub fn is_distribution_build() -> bool {
        identity::expected_digest().is_some()
    }

    /// Admits the complete installation surrounding the directly running compiler.
    ///
    /// # Errors
    /// Rejects absent build identity, unsafe topology, changed bytes or incomplete metadata.
    pub fn capture_current() -> Result<Self, Diagnostic> {
        let expected = identity::expected_digest().ok_or_else(|| {
            admission_error("this compiler has no prepared distribution identity")
        })?;
        let executable = std::env::current_exe()
            .map_err(|_| admission_error("running compiler path is unavailable"))?;
        let root = executable
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| admission_error("running compiler is outside an installation"))?;
        let mut tree = InstallationTree::capture(root)?;
        tree.capture_file("metadata/distribution.json", 262_144, true)?;
        if tree.digest("metadata/distribution.json")? != expected {
            return Err(admission_error("installed manifest differs from the compiled identity"));
        }
        let distribution = Distribution::parse(tree.bytes("metadata/distribution.json")?)?;
        if executable.file_name() != Path::new(distribution.cli_path()).file_name()
            || executable.parent().and_then(Path::file_name) != Some(std::ffi::OsStr::new("bin"))
        {
            return Err(admission_error(
                "compiler executable is outside the admitted bin directory",
            ));
        }
        for record in &distribution.files {
            let retain =
                record.role == "provider" || record.role == "metadata" || record.path == "VERSION";
            tree.capture_file(&record.path, record.size, retain)?;
            tree.matches(record)?;
        }
        for path in ["metadata/inventory.json", "metadata/checksums.sha256"] {
            tree.capture_file(path, 262_144, true)?;
            tree.mode_matches(path, 0o644)?;
        }
        integrity::validate_metadata(&mut tree, &distribution)?;
        if tree.identity(distribution.cli_path())? != &running_identity(&executable)? {
            return Err(admission_error(
                "installed CLI no longer identifies the running executable",
            ));
        }
        tree.revalidate()?;
        Ok(Self { root: root.to_owned(), tree, distribution })
    }

    /// Revalidates retained installation identities, complete topology and every file digest.
    ///
    /// # Errors
    /// Rejects replacement, byte mutation or a missing/extra installation entry.
    pub fn revalidate(&self) -> Result<(), Diagnostic> {
        self.tree.revalidate()
    }

    pub(crate) fn runtime(&self) -> Result<crate::runtime::NodeRuntimeCapability, Diagnostic> {
        self.revalidate()?;
        let path = self.distribution.node_path();
        let expected = crate::runtime::ExpectedRuntime {
            sha256: self.tree.digest(path)?.to_owned(),
            size: self.tree.size(path)?,
        };
        let runtime = crate::runtime::NodeRuntimeCapability::discover_authenticated(
            &self.root.join(path),
            &self.root,
            expected,
        )?;
        self.revalidate()?;
        Ok(runtime)
    }
}

fn running_identity(executable: &Path) -> Result<Handle, Diagnostic> {
    // This kernel-owned link denotes the already running image, including after a path rename.
    // It is not an archive-supplied path or a general no-follow exception.
    #[cfg(target_os = "linux")]
    let executable = {
        let _ = executable;
        Path::new("/proc/self/exe")
    };
    std::fs::File::open(executable)
        .and_then(Handle::from_file)
        .map_err(|_| admission_error("running executable identity cannot be retained"))
}

fn admission_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-C4220",
        None,
        message,
        "restore the independently verified complete distribution and retry from a stable installation",
    )
}
