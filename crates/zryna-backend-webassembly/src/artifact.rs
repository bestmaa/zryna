pub(super) const MAX_CONTROL_FLOW_WEBASSEMBLY_BYTES: usize = 32 * 1024 * 1024;

/// A complete core WebAssembly module that passed the pinned validator and the Zryna profile audit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedWebAssemblyArtifact {
    pub(super) bytes: Vec<u8>,
}

impl ValidatedWebAssemblyArtifact {
    /// Returns the exact validated module bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
