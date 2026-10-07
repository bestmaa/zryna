//! Distinct private transport for the compiler-generated shared String clone leaf.
//! Decoding grants raw claims only; the existing mandatory verifier remains the sole seal.
use super::{DecodedProgram, Failure, raw};
/// Distinct v3 domain, never accepted by the frozen v2 decoder.
pub const HEADER: &[u8] = b"ZRYNA-GENERIC-OWNED-IR-V3\0";
/// Private structural-clone transport version.
pub const VERSION: u32 = 3;
/// Encodes raw claims with the additional shared String loan clone opcode.
/// # Errors
/// Rejects inherited count, byte and aggregate child amplification limits.
pub fn encode(claim: &raw::Program) -> Result<Vec<u8>, Failure> {
    super::encode_for(claim, HEADER, VERSION, true)
}
/// Decodes v3 raw claims without constructing executable authority.
/// # Errors
/// Rejects foreign domains, unknown tags, malformed data and amplification.
pub fn decode(bytes: &[u8]) -> Result<DecodedProgram, Failure> {
    super::decode_for(bytes, HEADER, VERSION, true)
}
