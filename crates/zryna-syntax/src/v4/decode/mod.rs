use super::*;

pub(super) mod collections;
mod duplicate_keys;

pub(super) use duplicate_keys::reject_duplicate_json_keys;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyntaxDecodeError {
    ResponseTooLarge { actual: usize, limit: usize },
    InvalidSnapshot,
}
impl SyntaxDecodeError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ResponseTooLarge { .. } => "ZRYNA-F1401",
            Self::InvalidSnapshot => "ZRYNA-Y4001",
        }
    }
}
impl fmt::Display for SyntaxDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResponseTooLarge { actual, limit } => {
                write!(f, "syntax response contains {actual} bytes; the limit is {limit}")
            }
            Self::InvalidSnapshot => {
                f.write_str("syntax response is not exact bounded protocol-v4 JSON")
            }
        }
    }
}
impl std::error::Error for SyntaxDecodeError {}

/// Decodes one exact, resource-bounded protocol-v4 JSON response.
///
/// # Errors
///
/// Returns a stable decode failure when the byte ceiling is exceeded or the response is not the
/// closed protocol-v4 DTO grammar.
pub fn decode_snapshot(bytes: &[u8]) -> Result<RawProjectSyntaxSnapshot, SyntaxDecodeError> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(SyntaxDecodeError::ResponseTooLarge {
            actual: bytes.len(),
            limit: MAX_RESPONSE_BYTES,
        });
    }
    reject_duplicate_json_keys(bytes)?;
    serde_json::from_slice(bytes).map_err(|_| SyntaxDecodeError::InvalidSnapshot)
}
