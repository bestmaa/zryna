use zryna_source::UntrustedSpan;
use zryna_syntax::v5::RawExpressionKind;

use super::super::DeclarationIdentity;
use super::capabilities::Status;
use super::model::{Head, Kind, Scalar, Ty};
use super::{
    BodyTypeFailure, Checker, constraints, constructions, resources, substitution, type_resolution,
};

pub(super) fn construction(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    type_syntax: u32,
    elements: &[u32],
    fixed: bool,
    at: UntrustedSpan,
) -> Result<Option<Ty>, BodyTypeFailure> {
    let Some(ty) = type_resolution::source(checker, owner.module().index(), type_syntax) else {
        return Ok(None);
    };
    let Some(head) = substitution::head(&checker.tables, owner, ty)? else { return Ok(None) };
    let valid = match head.kind {
        Kind::FixedArray(length) if fixed => {
            if length as usize != elements.len() {
                if !constraints::relevant(&checker.tables, owner, ty)? {
                    constructions::legacy_error(
                        checker,
                        at,
                        format!(
                            "fixed-array constructor has {} elements but its type requires {length}",
                            elements.len()
                        ),
                        "provide exactly the fixed-array length",
                    );
                    return Ok(None);
                }
                constraints::mismatch(
                    checker,
                    at,
                    "fixed array element count differs from its exact length",
                );
            }
            true
        }
        Kind::Vec if !fixed => true,
        _ => false,
    };
    if !valid {
        constraints::mismatch(
            checker,
            at,
            "container constructor requires its exact container annotation",
        );
        return Ok(None);
    }
    for element in elements {
        let actual = checker.expression(owner, *element);
        let at = resources::raw_function(checker.context, owner).body.expressions
            [*element as usize]
            .span;
        if !constraints::require(
            checker,
            owner,
            head.children[0],
            actual,
            at,
            if fixed { "fixed-array element" } else { "container element" },
        )? {
            return Ok(None);
        }
    }
    Ok(Some(ty))
}

pub(super) fn unary(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    expression: u32,
    kind: &RawExpressionKind,
    value: u32,
    at: UntrustedSpan,
) -> Result<Option<Ty>, BodyTypeFailure> {
    let Some(ty) = checker.expression(owner, value) else { return Ok(None) };
    let Some(head) = substitution::head(&checker.tables, owner, ty)? else { return Ok(None) };
    let result = match kind {
        RawExpressionKind::Clone { .. } => {
            let capability = checker.capabilities.query(&checker.tables, owner, ty, true, false)?;
            if capability == Status::False {
                let assuming =
                    checker.capabilities.query(&checker.tables, owner, ty, true, true)?;
                if assuming == Status::True {
                    constraints::opaque(checker, at, "Clone capability");
                } else {
                    let span = checker.span(at);
                    checker.constraints.at(
                        "ZRYNA-M3008",
                        span,
                        "value does not provide Clone",
                        "clone a value with the complete Clone capability",
                    );
                }
                return Ok(None);
            }
            if capability == Status::Unknown {
                return Ok(None);
            }
            return Ok(Some(ty));
        }
        RawExpressionKind::Shared { .. } => Head::unary(Kind::Shared, ty),
        RawExpressionKind::Borrow { .. } => Head::unary(Kind::Borrow, ty),
        RawExpressionKind::BorrowMut { .. } => Head::unary(Kind::BorrowMut, ty),
        RawExpressionKind::Downgrade { .. } => {
            if matches!(head.kind, Kind::Parameter(_)) {
                constraints::opaque(checker, at, "Shared handle capability");
                return Ok(None);
            }
            if head.kind != Kind::Shared {
                constraints::mismatch(checker, at, "downgrade requires a Shared handle");
                return Ok(None);
            }
            Head::unary(Kind::Weak, head.children[0].ok_or(BodyTypeFailure::InternalFailure)?)
        }
        _ => return Err(BodyTypeFailure::InternalFailure),
    };
    Ok(Some(substitution::issue_expression(checker, owner, expression, result)?))
}

pub(super) fn push(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    vector: u32,
    value: u32,
    at: UntrustedSpan,
) -> Result<Option<Ty>, BodyTypeFailure> {
    let Some(vector) = checker.expression(owner, vector) else { return Ok(None) };
    let Some(head) = substitution::head(&checker.tables, owner, vector)? else { return Ok(None) };
    if matches!(head.kind, Kind::Parameter(_)) {
        constraints::opaque(checker, at, "Vec capability");
        return Ok(None);
    }
    if head.kind != Kind::Vec {
        constraints::mismatch(checker, at, "vecPush requires a Vec value");
        return Ok(None);
    }
    let actual = checker.expression(owner, value);
    let at = resources::raw_function(checker.context, owner).body.expressions[value as usize].span;
    constraints::require(checker, owner, head.children[0], actual, at, "pushed element")?;
    Ok(actual.map(|_| Ty::scalar(Scalar::Unit)))
}
