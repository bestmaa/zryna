//! Bounded one-request transport for the separately isolated compiler facade.

#![forbid(unsafe_code)]

use std::io::Read;

use serde::Deserialize;
mod structure;

/// Inclusive complete UTF-8 JSON request budget, including whitespace and escaped source.
pub const MAX_REQUEST_BYTES: usize = 32_768;

/// Closed inert source request; no path, executable, target, grant or invocation is accepted.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRequest {
    version: u32,
    revision: u64,
    source: String,
}

impl SourceRequest {
    /// Returns the exact browser-owned revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the exact source without newline or Unicode normalization.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// Reads one complete bounded request and rejects malformed, extra or unsupported input.
///
/// Call this before discovering a compiler/provider. The external supervisor owns the deadline.
///
/// # Errors
/// Returns a stable transport policy reason, never a fabricated compiler diagnostic.
pub fn read_request(input: &mut impl Read) -> Result<SourceRequest, &'static str> {
    let mut bytes = Vec::new();
    input
        .take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "PLAYGROUND-REQUEST-IO")?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err("PLAYGROUND-REQUEST-LIMIT");
    }
    structure::validate(&bytes)?;
    let request: SourceRequest =
        serde_json::from_slice(&bytes).map_err(|_| "PLAYGROUND-REQUEST-SHAPE")?;
    if request.version != 1 {
        return Err("PLAYGROUND-REQUEST-VERSION");
    }
    if request.revision == 0
        || request.revision > (1_u64 << 53) - 1
        || request.source.len() > zryna_driver::MAX_BROWSER_SOURCE_BYTES
    {
        return Err("PLAYGROUND-REQUEST-LIMIT");
    }
    Ok(request)
}

#[cfg(test)]
mod tests;
