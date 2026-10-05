//! Bounded metadata followed by exact compiler-authenticated binary payloads.

use std::io::Write;

use super::{BrowserCompilation, RestrictedBrowserError};

const MAX_HEADER_BYTES: usize = 98_304;
const MAX_FRAME_BYTES: usize = 1_277_956;

impl BrowserCompilation {
    /// Writes a little-endian u32 metadata length, UTF-8 JSON, then ordered binary artifacts.
    ///
    /// The caller owns bounded capture and interruption. No pathname or publication is selected.
    ///
    /// # Errors
    /// Rejects complete-frame limits before writing, or returns an I/O failure.
    pub fn write_frame(&self, output: &mut impl Write) -> Result<(), RestrictedBrowserError> {
        let metadata = serde_json::to_vec(self)
            .map_err(|_| RestrictedBrowserError("PLAYGROUND-FRAME-INVARIANT"))?;
        let total = self
            .artifacts
            .iter()
            .try_fold(metadata.len() + 4, |sum, artifact| sum.checked_add(artifact.content.len()));
        if metadata.len() > MAX_HEADER_BYTES || total.is_none_or(|total| total > MAX_FRAME_BYTES) {
            return Err(RestrictedBrowserError("PLAYGROUND-OUTPUT-LIMIT"));
        }
        let length = u32::try_from(metadata.len())
            .map_err(|_| RestrictedBrowserError("PLAYGROUND-OUTPUT-LIMIT"))?;
        output
            .write_all(&length.to_le_bytes())
            .and_then(|()| output.write_all(&metadata))
            .map_err(|_| RestrictedBrowserError("PLAYGROUND-OUTPUT-IO"))?;
        for artifact in &self.artifacts {
            output
                .write_all(&artifact.content)
                .map_err(|_| RestrictedBrowserError("PLAYGROUND-OUTPUT-IO"))?;
        }
        output.flush().map_err(|_| RestrictedBrowserError("PLAYGROUND-OUTPUT-IO"))
    }
}
