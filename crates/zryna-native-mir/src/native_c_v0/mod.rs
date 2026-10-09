//! Independently admitted Linux x86-64 C machine plans, retaining genuine extension IR.

use std::fmt;
use zryna_diagnostics::Diagnostic;
use zryna_native_c_ir::{VerifiedNativeCProgram, contract::Span};

/// Exact physical INTEGER lanes and caller output-slot requirements.
pub mod abi;
/// Closed vocabulary already exposed by immutable machine/source views; no issuer factories.
pub mod contract {
    pub use zryna_native_c_ir::contract::*;
}
/// Compiler-private entry channels, separate from every public foreign C signature.
pub mod entry;
mod lower;
/// Explicit hostile machine claims; no raw value is a backend authority.
pub mod raw;
mod verify;
mod views;

pub use lower::{lower, lower_unverified};
pub use verify::verify;
pub use views::{VerifiedEffect, VerifiedFunction, VerifiedOperation};

/// Immutable source-bound native machine authority. Only independent verification constructs it.
///
/// ```compile_fail
/// let _ = zryna_native_mir::native_c_v0::VerifiedMirProgram {
///     program: todo!(), source: todo!()
/// };
/// ```
#[derive(Clone, Debug)]
pub struct VerifiedMirProgram {
    program: raw::Program,
    source: VerifiedNativeCProgram,
}

/// Atomic, bounded machine admission rejection with the original location when available.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirError {
    code: &'static str,
    detail: &'static str,
    span: Option<Span>,
}
impl MirError {
    /// Existing native C diagnostic category, never terminal-report C4108.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }
    /// Fixed rejected invariant.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
    /// Original source location; global/raw budget errors have none.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        self.span
    }
    /// Preserves category and authenticated source location.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        let help = "provide source-bound native C machine claims";
        match self.span {
            Some(span) => Diagnostic::error_at(self.code, span, self.detail, help),
            None => Diagnostic::error(self.code, None, self.detail, help),
        }
    }
    pub(crate) const fn new(code: &'static str, detail: &'static str) -> Self {
        Self { code, detail, span: None }
    }
    pub(crate) fn at(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}
impl fmt::Display for MirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.code, self.detail)
    }
}
impl std::error::Error for MirError {}

pub(crate) fn require(ok: bool, code: &'static str, detail: &'static str) -> Result<(), MirError> {
    if ok { Ok(()) } else { Err(MirError::new(code, detail)) }
}
