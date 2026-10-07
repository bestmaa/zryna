//! Private installed native-provider preparation, without CLI selection or target execution.
//!
//! This feature-gated issuer authenticates an isolated preparation installation against the
//! running image's compile-time descriptor/source binding. Initial executable authenticity must
//! still be established independently before execution. It does not admit production archives,
//! change an ordinary installed provider, or establish ordinary-default no-Node acceptance.

mod descriptor;
mod identity;
mod sources;

pub use sources::{InstalledNativeSources, InstalledNativeSummary, InstalledNativeSyntax};

use same_file::Handle;
use zryna_diagnostics::Diagnostic;

use crate::distribution::{filesystem::InstallationTree, running_identity};
use descriptor::{
    DESCRIPTOR, Descriptor, MAX_DESCRIPTOR_BYTES, MAX_EXECUTABLE_BYTES, MAX_LICENSE_BYTES, cli_path,
};

/// Retained authority for exactly one private native-provider preparation installation.
///
/// The constructor accepts no path, digest or caller assertion. Its only root is the running
/// image's admitted bin directory; the complete descriptor is bound into that image at compile
/// time. The installation contains only the executing proof image, descriptor and license.
/// Linux retains the kernel's executing image; the unchanged Windows helper opens the current
/// executable path. Neither path replaces independent initial executable authentication.
pub struct NativeInstallation {
    tree: InstallationTree,
    running_image: Handle,
}

impl NativeInstallation {
    /// Captures the directly executing preparation installation without issuing a Node runtime.
    ///
    /// # Errors
    /// Rejects missing compile-time authority, changed identity/digests, unsafe topology, links,
    /// incomplete or extra files, foreign source/host/protocol metadata and existing size bounds.
    pub fn capture_current() -> Result<Self, Diagnostic> {
        let binding = identity::compiled()
            .ok_or_else(|| reject("running image has no private native installation binding"))?;
        let executable = std::env::current_exe()
            .map_err(|_| reject("running private native image path is unavailable"))?;
        let expected = std::path::Path::new(cli_path());
        if executable.file_name() != expected.file_name()
            || executable.parent().and_then(std::path::Path::file_name)
                != Some(std::ffi::OsStr::new("bin"))
        {
            return Err(reject("running image is outside the private native bin directory"));
        }
        let root = executable
            .parent()
            .and_then(std::path::Path::parent)
            .ok_or_else(|| reject("private native installation root is unavailable"))?;
        let mut tree = InstallationTree::capture(root)?;
        tree.capture_file(DESCRIPTOR, MAX_DESCRIPTOR_BYTES, true)?;
        tree.mode_matches(DESCRIPTOR, 0o600)?;
        if tree.digest(DESCRIPTOR)? != binding.descriptor_sha256 {
            return Err(reject("private native descriptor differs from the executing image"));
        }
        let descriptor = Descriptor::parse(tree.bytes(DESCRIPTOR)?, &binding)?;
        tree.capture_file("LICENSE", MAX_LICENSE_BYTES, true)?;
        tree.mode_matches("LICENSE", 0o600)?;
        if tree.digest("LICENSE")? != descriptor.license_sha256 {
            return Err(reject("private native installation license bytes differ"));
        }
        tree.capture_file(cli_path(), MAX_EXECUTABLE_BYTES, false)?;
        tree.mode_matches(cli_path(), 0o700)?;
        let running_image = running_identity(&executable)?;
        let installation = Self { tree, running_image };
        installation.revalidate()?;
        Ok(installation)
    }

    /// Revalidates complete retained installation identities and file bytes.
    ///
    /// # Errors
    /// Rejects a replaced executing path, stale directory/file identity, mutation or extra entry.
    pub fn revalidate(&self) -> Result<(), Diagnostic> {
        if self.tree.identity(cli_path())? != &self.running_image {
            return Err(reject("private native image no longer identifies the executing image"));
        }
        self.tree.revalidate()
    }
}

fn reject(message: impl Into<String>) -> Diagnostic {
    crate::distribution::admission_error(message)
}
