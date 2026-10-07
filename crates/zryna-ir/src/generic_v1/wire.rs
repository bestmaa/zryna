//! Separately versioned little-endian successor wire admission. No older protocol is widened.

use super::{Failure, raw, reject, reserve};

mod decoding;
mod encoding;

/// Exact domain prefix followed by a little-endian schema version.
pub const HEADER: &[u8] = b"ZRYNA-GENERIC-IR-V1\0";
/// First independent successor schema.
pub const VERSION: u32 = 1;
/// Complete message byte ceiling before allocation or traversal.
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
/// Total decoded vector children, preventing nested allocation amplification.
pub const MAX_CHILDREN: usize = 1_048_576;

/// Bounded wire-admitted claims; this remains untrusted and cannot execute.
#[derive(Debug)]
pub struct DecodedProgram(pub(super) raw::Program);

impl DecodedProgram {
    /// Immutable untrusted claims for independent verification.
    #[must_use]
    pub const fn claims(&self) -> &raw::Program {
        &self.0
    }
}

/// Encodes raw claims in the separately frozen successor domain, granting no authority.
///
/// # Errors
/// Rejects inherited raw graph budgets or a wire message exceeding its byte ceiling.
pub fn encode(program: &raw::Program) -> Result<Vec<u8>, Failure> {
    super::inventory::preflight(program)?;
    encoding::encode(program)
}

/// Decodes only the complete successor domain/version with bounded allocation and exact tags.
///
/// # Errors
/// Rejects truncation, unknown tags, noncanonical bool/UTF-8, amplification and trailing bytes.
pub fn decode(bytes: &[u8]) -> Result<DecodedProgram, Failure> {
    if bytes.len() > MAX_BYTES {
        return Err(super::budget("successor wire bytes exceed 32 MiB"));
    }
    let program = decoding::decode(bytes)?;
    super::inventory::preflight(&program)?;
    Ok(DecodedProgram(program))
}

fn invalid() -> Failure {
    reject("malformed separately versioned successor IR wire claim")
}
