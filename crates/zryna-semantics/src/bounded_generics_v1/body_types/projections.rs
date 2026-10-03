use zryna_source::UntrustedSpan;
use zryna_syntax::v4::RawIdentifierSyntax;

use super::super::DeclarationIdentity;
use super::model::{Head, Kind, Scalar, Ty};
use super::{BodyTypeFailure, Checker, constraints, signatures, substitution};

pub(super) fn base(
    checker: &Checker<'_, '_>,
    owner: DeclarationIdentity,
    ty: Ty,
) -> Result<Option<(Head, Option<Kind>)>, BodyTypeFailure> {
    let Some(head) = substitution::head(&checker.tables, owner, ty)? else { return Ok(None) };
    if matches!(head.kind, Kind::Borrow | Kind::BorrowMut) {
        let child = head.children[0].ok_or(BodyTypeFailure::InternalFailure)?;
        Ok(substitution::head(&checker.tables, owner, child)?.map(|child| (child, Some(head.kind))))
    } else {
        Ok(Some((head, None)))
    }
}

fn result(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    expression: u32,
    ty: Ty,
    access: Option<Kind>,
) -> Result<Option<Ty>, BodyTypeFailure> {
    Ok(Some(match access {
        Some(kind) => {
            substitution::issue_expression(checker, owner, expression, Head::unary(kind, ty))?
        }
        None => ty,
    }))
}

pub(super) fn field(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    expression: u32,
    value: u32,
    field: &RawIdentifierSyntax,
    at: UntrustedSpan,
) -> Result<Option<Ty>, BodyTypeFailure> {
    let Some(ty) = checker.expression(owner, value) else { return Ok(None) };
    let Some((head, access)) = base(checker, owner, ty)? else { return Ok(None) };
    if matches!(head.kind, Kind::Parameter(_)) {
        constraints::opaque(checker, at, "field projection");
        return Ok(None);
    }
    let environment = signatures::nominal_environment(checker, owner, head)?;
    let Some(ty) = signatures::field(checker.context, head, &field.text, environment) else {
        constraints::mismatch(checker, field.span, "projection names no original struct field");
        return Ok(None);
    };
    result(checker, owner, expression, ty, access)
}

pub(super) fn index(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    expression: u32,
    value: u32,
    index: u32,
    at: UntrustedSpan,
) -> Result<Option<Ty>, BodyTypeFailure> {
    let actual = checker.expression(owner, index);
    if let Some(actual) = actual {
        if substitution::head(&checker.tables, owner, actual)?
            .is_some_and(|head| matches!(head.kind, Kind::Parameter(_)))
        {
            constraints::opaque(checker, at, "integer indexing");
        } else {
            constraints::require(
                checker,
                owner,
                Some(Ty::scalar(Scalar::I32)),
                Some(actual),
                at,
                "index",
            )?;
        }
    }
    let Some(ty) = checker.expression(owner, value) else { return Ok(None) };
    let Some((head, access)) = base(checker, owner, ty)? else { return Ok(None) };
    if matches!(head.kind, Kind::Parameter(_)) {
        constraints::opaque(checker, at, "indexable capability");
        return Ok(None);
    }
    if !matches!(head.kind, Kind::Vec | Kind::FixedArray(_)) {
        constraints::mismatch(checker, at, "indexing requires a Vec or FixedArray");
        return Ok(None);
    }
    let child = head.children[0].ok_or(BodyTypeFailure::InternalFailure)?;
    if actual.is_none() {
        return Ok(None);
    }
    result(checker, owner, expression, child, access)
}
