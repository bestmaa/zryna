//! Independently verified native-requirement Universal IR extension.
//!
//! This component retains genuine source/body/private authorities. It provides no MIR,
//! object, runtime allocation, native linking, public selector or execution permission.
#![forbid(unsafe_code)]

use std::fmt;
use zryna_diagnostics::Diagnostic;
use zryna_semantics::native_c_v0::body::VerifiedPrivateBoundaries;
use zryna_source::Span;

/// Closed record vocabulary already exposed by immutable IR views; no issuer constructors.
pub mod contract;
mod lower;
/// Closed untrusted program claims and the independent hostile-input seam.
pub mod raw;
mod verify;
mod views;

pub use lower::{lower, lower_unverified};
pub use verify::verify;
pub use views::{VerifiedEffect, VerifiedFunction, VerifiedOperation, VerifiedValue};

/// Opaque IR authority; only mandatory independent verification constructs it.
///
/// ```compile_fail
/// let _ = zryna_native_c_ir::VerifiedNativeCProgram { program: todo!(), authority: todo!() };
/// ```
#[derive(Clone, Debug)]
pub struct VerifiedNativeCProgram {
    program: raw::Program,
    authority: VerifiedPrivateBoundaries,
}

/// One atomic rejection; no partial program is retained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrError {
    code: &'static str,
    detail: &'static str,
    span: Option<Span>,
}
impl IrError {
    /// Fixed native-C producing diagnostic category. C4108 is not an ordinary rejection.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }
    /// Stable rejected requirement.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
    /// Original authenticated source location, absent for global/raw identity failures.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        self.span
    }
    /// Produces a diagnostic preserving the exact category and source issuer.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        let guidance = "provide complete source-bound native C IR";
        match self.span {
            Some(span) => Diagnostic::error_at(self.code, span, self.detail, guidance),
            None => Diagnostic::error(self.code, None, self.detail, guidance),
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
impl fmt::Display for IrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.code, self.detail)
    }
}
impl std::error::Error for IrError {}

pub(crate) fn require(
    condition: bool,
    code: &'static str,
    detail: &'static str,
) -> Result<(), IrError> {
    if condition { Ok(()) } else { Err(IrError::new(code, detail)) }
}
