//! Isolated native C v0 untrusted declaration decoding.
//!
//! This module does not reinterpret syntax protocols v2/v3/v4, authenticate source nodes,
//! verify declaration policies, construct IR/MIR, admit a profile or authorize native execution.
//! Successful decoding returns raw claims requiring independent source and semantic verification.

use std::fmt;

mod validation;
mod wire;

/// Public untrusted declaration records and closed wire tags.
pub mod raw;

/// Exact admitted native C target.
pub const TARGET: &str = "x86_64-unknown-linux-gnu";
/// Maximum complete declaration wire bytes, including the terminal LF.
pub const MAX_WIRE_BYTES: usize = 1_048_576;
/// Maximum object/array nesting, with the root at depth one.
pub const MAX_WIRE_DEPTH: usize = 16;
/// Maximum aggregate string-value UTF-8 bytes, counting every occurrence.
pub const MAX_STRING_BYTES: usize = 65_536;

/// Stable rejection at the untrusted declaration decoding boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodeError {
    code: &'static str,
    detail: &'static str,
}

impl DecodeError {
    /// Returns the fixed producing diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the fixed rejected category or resource metric.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.code, self.detail)
    }
}

impl std::error::Error for DecodeError {}

const fn error(code: &'static str, detail: &'static str) -> DecodeError {
    DecodeError { code, detail }
}

fn require(condition: bool, code: &'static str, detail: &'static str) -> Result<(), DecodeError> {
    if condition { Ok(()) } else { Err(error(code, detail)) }
}

/// Decodes the exact bounded canonical declaration wire into untrusted records.
///
/// No digest, source span, library identity, owner, release, output or body claim is authenticated
/// by this operation. Consumers must not substitute these raw records for sealed compiler values.
/// The selected target is independently supplied; the declaration cannot select a host target.
///
/// # Errors
///
/// Rejects invalid UTF-8/JSON, duplicate keys, noncanonical encoding, closed-shape violations,
/// exact resource-limit overflow and unsupported target selection. It retains no partial set.
pub fn decode(bytes: &[u8], selected_target: &str) -> Result<raw::DeclarationSet, DecodeError> {
    let document = wire::decode(bytes)?;
    let string_budget = wire::check_string_budget(&document);
    let declarations =
        serde_json::from_value(document).map_err(|_| error("ZRYNA-C4101", "closed-shape"))?;
    validation::check(&declarations)?;
    string_budget?;
    require(selected_target == TARGET && declarations.target == TARGET, "ZRYNA-C4103", "target")?;
    Ok(declarations)
}

#[cfg(test)]
mod tests;
