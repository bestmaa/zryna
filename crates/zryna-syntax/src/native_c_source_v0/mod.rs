//! Independent authentication of the complete restricted native C source grammar.
//!
//! This distinct syntax authority does not reinterpret protocols v2/v3/v4. It proves exact
//! immutable source bytes, complete grammar and intrinsic nodes, not names, types, ownership,
//! total function bodies, IR/MIR, library safety or execution support.

use std::fmt;

mod authentication;
mod lexer;
mod parser;
pub mod raw;

pub use authentication::{
    AuthenticatedFile, AuthenticatedForeignSources, IntrinsicSite, authenticate_sources,
};

/// Maximum complete source inventory in this restricted extension.
pub const MAX_FILES: usize = 256;
/// Maximum aggregate source bytes in this restricted extension.
pub const MAX_SOURCE_BYTES: usize = 8 * 1_024 * 1_024;
/// Maximum tokens in one complete file.
pub const MAX_TOKENS: usize = 262_144;
/// Maximum functions in one file.
pub const MAX_FUNCTIONS: usize = 256;
/// Maximum complete functions in one project.
pub const MAX_PROJECT_FUNCTIONS: usize = 4_096;
/// Maximum expressions across the complete restricted project.
pub const MAX_PROJECT_EXPRESSIONS: usize = 262_144;
/// Maximum statements across the complete restricted project.
pub const MAX_PROJECT_STATEMENTS: usize = 65_536;
/// Maximum statements in one function.
pub const MAX_STATEMENTS: usize = 4_096;
/// Maximum expressions in one function arena.
pub const MAX_EXPRESSIONS: usize = 16_384;
/// Maximum expression depth, including additions and nested arguments.
pub const MAX_EXPRESSION_DEPTH: usize = 128;

/// Single fail-closed source authentication rejection; no partial authority is returned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceAuthError {
    code: &'static str,
    detail: &'static str,
    range: raw::Range,
}

impl SourceAuthError {
    /// Fixed producing diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }
    /// Rejected grammar category or budget metric.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
    /// Offending byte range, or the current end position.
    #[must_use]
    pub const fn range(&self) -> raw::Range {
        self.range
    }
}

impl fmt::Display for SourceAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.code, self.detail)
    }
}
impl std::error::Error for SourceAuthError {}

fn error(detail: &'static str, range: raw::Range) -> SourceAuthError {
    SourceAuthError { code: "ZRYNA-C4106", detail, range }
}
fn limit(detail: &'static str, range: raw::Range) -> SourceAuthError {
    SourceAuthError { code: "ZRYNA-C4107", detail, range }
}

#[cfg(test)]
mod tests;
