use zryna_syntax::v5::RawDataDeclarationKind;

use super::super::{DeclarationContext, DeclarationIdentity};
use super::model::{Head, Kind, Ty};
use super::{BodyTypeFailure, Checker, substitution};

pub(super) fn data<'a>(
    context: &'a DeclarationContext<'_>,
    owner: DeclarationIdentity,
) -> &'a RawDataDeclarationKind {
    &context.syntax().files()[owner.module().index() as usize].data_declarations
        [owner.source_index() as usize]
        .kind
}

pub(super) fn nominal_environment(
    checker: &mut Checker<'_, '_>,
    function: DeclarationIdentity,
    head: Head,
) -> Result<u32, BodyTypeFailure> {
    match head.kind {
        Kind::Nominal(owner) => substitution::environment(checker, function, owner, head.children),
        _ => Ok(0),
    }
}

pub(super) fn field(
    context: &DeclarationContext<'_>,
    head: Head,
    name: &str,
    environment: u32,
) -> Option<Ty> {
    let Kind::Nominal(owner) = head.kind else { return None };
    let RawDataDeclarationKind::Struct { fields, .. } = data(context, owner) else { return None };
    let field = fields.iter().find(|field| field.name.text == name)?;
    Some(substitution::source(owner, field.type_syntax, environment))
}

pub(super) fn variant_count(context: &DeclarationContext<'_>, head: Head) -> Option<usize> {
    match head.kind {
        Kind::Option | Kind::Result => Some(2),
        Kind::Nominal(owner) => match data(context, owner) {
            RawDataDeclarationKind::Enum { variants, .. } => Some(variants.len()),
            RawDataDeclarationKind::Struct { .. } => None,
        },
        _ => None,
    }
}

/// Returns the original ordinal and the exact optional payload type.
pub(super) fn variant(
    context: &DeclarationContext<'_>,
    head: Head,
    name: &str,
    environment: u32,
) -> Option<(usize, Option<Ty>)> {
    match head.kind {
        Kind::Option => match name {
            "none" => Some((0, None)),
            "some" => Some((1, head.children[0])),
            _ => None,
        },
        Kind::Result => match name {
            "ok" => Some((0, head.children[0])),
            "err" => Some((1, head.children[1])),
            _ => None,
        },
        Kind::Nominal(owner) => {
            let RawDataDeclarationKind::Enum { variants, .. } = data(context, owner) else {
                return None;
            };
            let (ordinal, variant) =
                variants.iter().enumerate().find(|(_, variant)| variant.name.text == name)?;
            Some((
                ordinal,
                variant.payload_type.map(|id| substitution::source(owner, id, environment)),
            ))
        }
        _ => None,
    }
}
