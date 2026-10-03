use zryna_diagnostics::Diagnostic;
use zryna_source::{Span, UntrustedSpan};

use super::model::{Kind, Ty};
use super::{BodyTypeFailure, Checker, equality, resources, substitution};

/// Source-only candidates have empty instance keys and witnesses. The canonical tuple is
/// therefore the authenticated file/start/end/code tuple; duplicate tuples consume no slot.
pub(super) struct Lane {
    candidates: Vec<Diagnostic>,
}

impl Lane {
    pub(super) fn new() -> Result<Self, BodyTypeFailure> {
        Ok(Self { candidates: resources::reserve(256)? })
    }

    pub(super) fn at(
        &mut self,
        code: &str,
        span: Span,
        message: impl Into<String>,
        guidance: &str,
    ) {
        let key = |diagnostic: &Diagnostic| {
            let at = diagnostic.primary_span().expect("source-only type candidate");
            (at.file().index(), at.start(), at.end())
        };
        let candidate = Diagnostic::error_at(code, span, message, guidance);
        let Err(index) = self.candidates.binary_search_by(|existing| {
            key(existing).cmp(&key(&candidate)).then_with(|| existing.code().cmp(candidate.code()))
        }) else {
            return;
        };
        if index < 256 {
            if self.candidates.len() == 256 {
                self.candidates.pop();
            }
            self.candidates.insert(index, candidate);
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    pub(super) fn finish(mut self) -> Vec<Diagnostic> {
        if self.candidates.len() == 256 {
            let extra = self.candidates.pop().expect("canonical first extra");
            self.candidates.push(Diagnostic::error_at(
                "ZRYNA-M7201",
                extra.primary_span().expect("source terminal"),
                "body type analysis exceeds 255 ordinary diagnostics; first extra 256",
                "fix the retained errors before compiling again",
            ));
        }
        self.candidates
    }
}

pub(super) fn require(
    checker: &mut Checker<'_, '_>,
    owner: super::super::DeclarationIdentity,
    expected: Option<Ty>,
    actual: Option<Ty>,
    at: UntrustedSpan,
    what: &str,
) -> Result<bool, BodyTypeFailure> {
    require_at(checker, owner, expected, actual, at, what, false)
}

pub(super) fn require_generic(
    checker: &mut Checker<'_, '_>,
    owner: super::super::DeclarationIdentity,
    expected: Option<Ty>,
    actual: Option<Ty>,
    at: UntrustedSpan,
    what: &str,
) -> Result<bool, BodyTypeFailure> {
    require_at(checker, owner, expected, actual, at, what, true)
}

fn require_at(
    checker: &mut Checker<'_, '_>,
    owner: super::super::DeclarationIdentity,
    expected: Option<Ty>,
    actual: Option<Ty>,
    at: UntrustedSpan,
    what: &str,
    generic_use: bool,
) -> Result<bool, BodyTypeFailure> {
    let (Some(expected), Some(actual)) = (expected, actual) else {
        // A pending invalid argument has no successful equality judgement.
        return Ok(false);
    };
    let equal = equality::equal(&checker.tables, owner, expected, actual, &mut checker.equality)?;
    if !equal {
        let generic = generic_use
            || relevant(&checker.tables, owner, expected)?
            || relevant(&checker.tables, owner, actual)?;
        let span = checker.span(at);
        if generic {
            checker.constraints.at(
                "ZRYNA-M7006",
                span,
                format!("{what} has a different exact symbolic type"),
                "use the exact declared type and ordered explicit arguments",
            );
        } else {
            checker.constraints.at(
                "ZRYNA-M3007",
                span,
                format!("{what} has a different exact aggregate type"),
                "use a value with the exact declared type",
            );
        }
    }
    Ok(equal)
}

pub(super) fn relevant(
    tables: &super::model::Tables,
    owner: super::super::DeclarationIdentity,
    ty: Ty,
) -> Result<bool, BodyTypeFailure> {
    let mut pending = resources::reserve(129)?;
    pending.push(ty);
    while let Some(ty) = pending.pop() {
        let Some(head) = substitution::head(tables, owner, ty)? else {
            continue;
        };
        if matches!(head.kind, Kind::Parameter(_) | Kind::Option | Kind::Result)
            || matches!(head.kind, Kind::Nominal(_)) && head.children[0].is_some()
        {
            return Ok(true);
        }
        for child in head.children.into_iter().flatten() {
            pending.try_reserve(1).map_err(|_| BodyTypeFailure::AllocationFailure)?;
            pending.push(child);
        }
    }
    Ok(false)
}

pub(super) fn opaque(checker: &mut Checker<'_, '_>, at: UntrustedSpan, operation: &str) {
    let span = checker.span(at);
    checker.opaque.at(
        "ZRYNA-M7002",
        span,
        format!("opaque ZrynaValue parameter does not provide {operation}"),
        "use only operations admitted by the declared bound",
    );
}

pub(super) fn mismatch(checker: &mut Checker<'_, '_>, at: UntrustedSpan, message: &str) {
    let span = checker.span(at);
    checker.constraints.at(
        "ZRYNA-M7006",
        span,
        message,
        "follow the original declaration's exact value signature",
    );
}
