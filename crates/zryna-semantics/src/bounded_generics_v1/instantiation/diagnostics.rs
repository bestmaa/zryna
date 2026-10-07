//! Bounded canonical rejection candidates for closed discovery.

use std::cmp::Ordering;

use super::{BodyTypeContext, InstantiationFailure, copy_bytes};
use zryna_diagnostics::Diagnostic;
use zryna_source::{Span, UntrustedSpan};

const MAX_DIAGNOSTICS: usize = 256;

#[derive(Debug)]
struct Candidate {
    diagnostic: Diagnostic,
    key: Vec<u8>,
    witness: Vec<usize>,
}

#[derive(Debug, Default)]
pub(super) struct Errors {
    candidates: Vec<Candidate>,
}

impl Errors {
    pub(super) fn at(
        &mut self,
        bodies: &BodyTypeContext<'_, '_>,
        code: &str,
        at: Option<UntrustedSpan>,
        key: &[u8],
        witness: &[usize],
        message: String,
    ) -> Result<(), InstantiationFailure> {
        let span = at
            .map(|at| bodies.declarations().sources().verify_span(at))
            .transpose()
            .map_err(|_| InstantiationFailure::InternalFailure)?;
        let guidance = "use the admitted bounded closed-instantiation graph";
        let diagnostic = match span {
            Some(span) => Diagnostic::error_at(code, span, message, guidance),
            None => Diagnostic::error(code, None, message, guidance),
        };
        let mut numeric = super::reserve(witness.len())?;
        numeric.extend_from_slice(witness);
        self.insert(Candidate { diagnostic, key: copy_bytes(key)?, witness: numeric })
    }

    fn insert(&mut self, candidate: Candidate) -> Result<(), InstantiationFailure> {
        let Err(position) = self.candidates.binary_search_by(|row| compare(row, &candidate)) else {
            return Ok(());
        };
        if position < MAX_DIAGNOSTICS {
            self.candidates.try_reserve(1).map_err(|_| InstantiationFailure::AllocationFailure)?;
            if self.candidates.len() == MAX_DIAGNOSTICS {
                self.candidates.pop();
            }
            self.candidates.insert(position, candidate);
        }
        Ok(())
    }

    pub(super) fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    pub(super) fn finish(mut self) -> Result<Vec<Diagnostic>, InstantiationFailure> {
        if self.candidates.len() == MAX_DIAGNOSTICS {
            let extra = self.candidates.last_mut().ok_or(InstantiationFailure::InternalFailure)?;
            let message = format!(
                "instantiation diagnostics limit 255; rejected count 256; canonical offending key {:?}; numeric witness {:?}",
                extra.key, extra.witness
            );
            let guidance = "fix the retained errors before compiling again";
            extra.diagnostic = match extra.diagnostic.primary_span() {
                Some(span) => Diagnostic::error_at("ZRYNA-M7201", span, message, guidance),
                None => Diagnostic::error("ZRYNA-M7201", None, message, guidance),
            };
        }
        let mut result = super::reserve(self.candidates.len())?;
        result.extend(self.candidates.into_iter().map(|row| row.diagnostic));
        Ok(result)
    }
}

fn compare(left: &Candidate, right: &Candidate) -> Ordering {
    let location = |diagnostic: &Diagnostic| {
        diagnostic.primary_span().map(|span: Span| (span.file().index(), span.start(), span.end()))
    };
    let location_order = match (location(&left.diagnostic), location(&right.diagnostic)) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    };
    location_order
        .then_with(|| left.diagnostic.code().cmp(right.diagnostic.code()))
        .then_with(|| left.key.cmp(&right.key))
        .then_with(|| left.witness.cmp(&right.witness))
}
