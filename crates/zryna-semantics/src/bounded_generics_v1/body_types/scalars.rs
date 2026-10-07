use zryna_source::UntrustedSpan;
use zryna_syntax::v5::RawExpressionKind;

use super::super::DeclarationIdentity;
use super::model::{Kind, Scalar, Ty};
use super::{BodyTypeFailure, Checker, constraints, substitution};

pub(super) fn integer(
    checker: &mut Checker<'_, '_>,
    spelling: &str,
    at: UntrustedSpan,
) -> Option<Ty> {
    if spelling.parse::<i32>().is_ok() {
        Some(Ty::scalar(Scalar::I32))
    } else {
        let span = checker.span(at);
        checker.constraints.at(
            "ZRYNA-M3008",
            span,
            format!("integer literal '{spelling}' is outside i32"),
            "use a decimal i32 literal",
        );
        None
    }
}

pub(super) fn operation(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    kind: &RawExpressionKind,
    at: UntrustedSpan,
    lhs: u32,
    rhs: Option<u32>,
) -> Result<Option<Ty>, BodyTypeFailure> {
    let left = checker.expression(owner, lhs);
    let right = rhs.and_then(|rhs| checker.expression(owner, rhs));
    // Inspect each independently known operand even when the other has invalid arguments.
    let mut opaque = false;
    for ty in [left, right].into_iter().flatten() {
        if substitution::head(&checker.tables, owner, ty)?
            .is_some_and(|head| matches!(head.kind, Kind::Parameter(_)))
        {
            opaque = true;
        }
    }
    if opaque {
        constraints::opaque(checker, at, "scalar arithmetic or comparison");
        return Ok(None);
    }
    let comparison =
        matches!(kind, RawExpressionKind::Equal { .. } | RawExpressionKind::NotEqual { .. });
    let boolean = comparison
        || matches!(
            kind,
            RawExpressionKind::LessThan { .. }
                | RawExpressionKind::LessEqual { .. }
                | RawExpressionKind::GreaterThan { .. }
                | RawExpressionKind::GreaterEqual { .. }
        );
    if comparison {
        if !constraints::require(checker, owner, left, right, at, "comparison")? {
            return Ok(None);
        }
        if let Some(left) = left
            && let Some(head) = substitution::head(&checker.tables, owner, left)?
            && !matches!(head.kind, Kind::Scalar(Scalar::Bool | Scalar::I32))
        {
            let span = checker.span(at);
            checker.constraints.at(
                "ZRYNA-M3008",
                span,
                "equality is scalar-only in aggregate M3",
                "compare bool or i32 projections rather than whole aggregates",
            );
        }
    } else {
        let expected = Some(Ty::scalar(Scalar::I32));
        let what = if matches!(kind, RawExpressionKind::Negation { .. }) {
            "negation operand"
        } else if boolean {
            "relational operand"
        } else {
            "left operand"
        };
        constraints::require(checker, owner, expected, left, at, what)?;
        if rhs.is_some() {
            constraints::require(
                checker,
                owner,
                expected,
                right,
                at,
                if boolean { "relational operand" } else { "right operand" },
            )?;
        }
    }
    if left.is_none() || rhs.is_some() && right.is_none() {
        return Ok(None);
    }
    Ok(Some(Ty::scalar(if boolean { Scalar::Bool } else { Scalar::I32 })))
}
