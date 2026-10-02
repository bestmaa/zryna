use std::cmp::Ordering;

use zryna_diagnostics::Diagnostic;
use zryna_source::Span;

const MAX_DIAGNOSTICS: usize = 256;

#[derive(Debug)]
struct Candidate {
    diagnostic: Diagnostic,
    witness: Vec<usize>,
}

/// Retains the first canonical 256 distinct candidates without allocating a full error inventory.
#[derive(Debug, Default)]
pub(super) struct Errors {
    candidates: Vec<Candidate>,
}

impl Errors {
    pub(super) fn at(
        &mut self,
        code: &str,
        span: Span,
        message: impl Into<String>,
        guidance: &str,
    ) {
        self.push(Candidate {
            diagnostic: Diagnostic::error_at(code, span, message, guidance),
            witness: Vec::new(),
        });
    }

    pub(super) fn global(
        &mut self,
        code: &str,
        message: impl Into<String>,
        guidance: &str,
        witness: Vec<usize>,
    ) {
        self.push(Candidate {
            diagnostic: Diagnostic::error(code, None, message, guidance),
            witness,
        });
    }

    fn push(&mut self, candidate: Candidate) {
        let Err(index) = self.candidates.binary_search_by(|existing| compare(existing, &candidate))
        else {
            return;
        };
        if index < MAX_DIAGNOSTICS {
            if self.candidates.len() == MAX_DIAGNOSTICS {
                self.candidates.pop();
            }
            self.candidates.insert(index, candidate);
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    pub(super) fn finish(mut self) -> Vec<Diagnostic> {
        if self.candidates.len() == MAX_DIAGNOSTICS {
            let first_extra = self.candidates.pop().expect("complete diagnostic prefix");
            let message = "declaration analysis exceeds 255 ordinary diagnostics; first extra 256";
            let guidance = "fix the retained errors before compiling again";
            let terminal = first_extra.diagnostic.primary_span().map_or_else(
                || Diagnostic::error("ZRYNA-M7201", None, message, guidance),
                |at| Diagnostic::error_at("ZRYNA-M7201", at, message, guidance),
            );
            self.candidates.push(Candidate { diagnostic: terminal, witness: vec![256, 255, 256] });
        }
        self.candidates.into_iter().map(|candidate| candidate.diagnostic).collect()
    }
}

fn compare(left: &Candidate, right: &Candidate) -> Ordering {
    let location = |diagnostic: &Diagnostic| {
        diagnostic.primary_span().map(|span| (span.file().index(), span.start(), span.end()))
    };
    let source_order = match (location(&left.diagnostic), location(&right.diagnostic)) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    };
    source_order
        .then_with(|| left.diagnostic.code().cmp(right.diagnostic.code()))
        .then_with(|| left.witness.cmp(&right.witness))
}
