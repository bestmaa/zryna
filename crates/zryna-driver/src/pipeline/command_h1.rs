//! A distinct command artifact uses the existing complete create-only transaction.

use super::{CommandFailure, ManifestTarget, Transaction};

impl Transaction {
    pub(crate) fn write_command_h1_artifact(
        &self,
        stem: &str,
        bytes: &[u8],
    ) -> Result<(), CommandFailure> {
        self.write_artifact(
            ManifestTarget::WasiCommand,
            "wasi-command-component-v1",
            stem,
            "wasm",
            bytes,
        )
        .map(|_| ())
    }
}
