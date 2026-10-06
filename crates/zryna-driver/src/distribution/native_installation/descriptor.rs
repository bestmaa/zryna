//! Closed private preparation schema, separate from production distribution admission.

use serde::Deserialize;
use zryna_diagnostics::Diagnostic;

use super::{identity::Binding, reject};
use crate::distribution::wire;

#[cfg(test)]
#[path = "descriptor_tests.rs"]
mod tests;

pub(super) const DESCRIPTOR: &str = "metadata/native-provider.json";
pub(super) const MAX_DESCRIPTOR_BYTES: u64 = 4096;
pub(super) const MAX_LICENSE_BYTES: u64 = 65_536;
pub(super) const MAX_EXECUTABLE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    repository: String,
    commit: String,
    tree: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Descriptor {
    format: String,
    version: String,
    source: Source,
    target: String,
    cli: String,
    protocols: Vec<u8>,
    pub(super) license_sha256: String,
}

pub(super) fn cli_path() -> &'static str {
    if cfg!(windows) {
        "bin/native-installation-proof.exe"
    } else {
        "bin/native-installation-proof"
    }
}

fn target() -> Result<&'static str, Diagnostic> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu")) {
        Ok("x86_64-unknown-linux-gnu")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")) {
        Ok("x86_64-pc-windows-msvc")
    } else {
        Err(reject("private native installation host is unsupported"))
    }
}

impl Descriptor {
    pub(super) fn parse(bytes: &[u8], binding: &Binding) -> Result<Self, Diagnostic> {
        if !u64::try_from(bytes.len()).is_ok_and(|size| size <= MAX_DESCRIPTOR_BYTES) {
            return Err(reject("private native descriptor exceeds 4096 bytes"));
        }
        let value: Self = wire::parse(bytes)?;
        if value.format != "zryna.native-installation-internal.v1"
            || value.version != env!("CARGO_PKG_VERSION")
            || value.source.repository != "https://github.com/zryna/zryna"
            || value.source.commit != binding.commit
            || value.source.tree != binding.tree
            || value.target != target()?
            || value.cli != cli_path()
            || value.protocols != [2, 3, 4]
            || !crate::distribution::manifest::hex(&value.license_sha256, 64)
        {
            return Err(reject("private native descriptor differs from its compiled authority"));
        }
        Ok(value)
    }
}
