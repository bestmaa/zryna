use zryna_syntax::v5::RawExpressionKind;

use super::super::DeclarationIdentity;
use super::model::{Head, Kind, Ty};
use super::{
    BodyTypeFailure, Checker, constraints, constructions, projections, resources, signatures,
    substitution, type_resolution,
};

pub(super) fn prepare(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    expression: u32,
) -> Result<(), BodyTypeFailure> {
    let function = resources::raw_function(checker.context, owner);
    let RawExpressionKind::Match { keyword_span, scrutinee, arms, .. } =
        &function.body.expressions[expression as usize].kind
    else {
        return Err(BodyTypeFailure::InternalFailure);
    };
    let Some(ty) = checker.expression(owner, *scrutinee) else { return Ok(()) };
    let Some((head, access)) = projections::base(checker, owner, ty)? else { return Ok(()) };
    if matches!(head.kind, Kind::Parameter(_)) {
        constraints::opaque(checker, *keyword_span, "enum match capability");
        return Ok(());
    }
    let Some(count) = signatures::variant_count(checker.context, head) else {
        constructions::variant_error(
            checker,
            *keyword_span,
            "match requires an original enum family",
        );
        return Ok(());
    };
    // One shared match environment substitutes every original user payload occurrence.
    let environment = signatures::nominal_environment(checker, owner, head)?;
    checker.tables.function_mut(owner).expressions[expression as usize].match_generic =
        head.children[0].is_some();
    let mut covered = resources::repeated(count, false)?;
    let mut valid = true;
    let start = checker.tables.function(owner).expressions[expression as usize].arm_start;
    for (index, arm) in arms.iter().enumerate() {
        let family = type_resolution::family(checker.context, owner, &arm.type_name.text);
        if family.map(|(kind, _)| kind) != Some(head.kind) {
            valid = false;
            constructions::variant_error(
                checker,
                arm.type_name.span,
                "match arm names a different exact family",
            );
            continue;
        }
        let Some((ordinal, payload)) =
            signatures::variant(checker.context, head, &arm.variant.text, environment)
        else {
            valid = false;
            constructions::variant_error(
                checker,
                arm.variant.span,
                "match arm names no original variant",
            );
            continue;
        };
        if covered[ordinal] {
            valid = false;
            constructions::variant_error(
                checker,
                arm.variant.span,
                "match arm repeats an original variant",
            );
        }
        covered[ordinal] = true;
        if arm.binding.is_some() != payload.is_some() {
            valid = false;
            constructions::variant_error(
                checker,
                arm.variant.span,
                "match payload binding differs from the original signature",
            );
        }
        let record = &mut checker.tables.function_mut(owner).arms[start + index];
        record.payload = payload;
        record.head = match (access, payload) {
            (Some(kind), Some(payload)) => Some(Head::unary(kind, payload)),
            _ => None,
        };
    }
    if covered.iter().any(|covered| !covered) {
        valid = false;
        constructions::variant_error(
            checker,
            *keyword_span,
            "match omits an original family variant",
        );
    }
    checker.tables.function_mut(owner).expressions[expression as usize].match_valid = valid;
    Ok(())
}

pub(super) fn payload(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    expression: u32,
    arm: usize,
) -> Result<Option<Ty>, BodyTypeFailure> {
    let records = checker.tables.function(owner);
    let index = records.expressions[expression as usize].arm_start + arm;
    let record = records.arms[index];
    let Some(head) = record.head else { return Ok(record.payload) };
    substitution::validate_children(&checker.tables, owner, head)?;
    let records = checker.tables.function_mut(owner);
    records.next_rank = records.next_rank.checked_add(1).ok_or(BodyTypeFailure::InternalFailure)?;
    records.arms[index].rank = records.next_rank;
    Ok(Some(Ty { origin: record.origin, environment: 0 }))
}

pub(super) fn finish(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    expression: u32,
) -> Result<Option<Ty>, BodyTypeFailure> {
    let function = resources::raw_function(checker.context, owner);
    let RawExpressionKind::Match { arms, .. } =
        &function.body.expressions[expression as usize].kind
    else {
        return Err(BodyTypeFailure::InternalFailure);
    };
    let mut common = None;
    let mut valid = checker.tables.function(owner).expressions[expression as usize].match_valid;
    for arm in arms {
        let actual = checker.expression(owner, arm.value);
        if actual.is_none() {
            valid = false;
        }
        if common.is_none() {
            common = actual;
        } else {
            let require =
                if checker.tables.function(owner).expressions[expression as usize].match_generic {
                    constraints::require_generic
                } else {
                    constraints::require
                };
            valid &= require(
                checker,
                owner,
                common,
                actual,
                function.body.expressions[arm.value as usize].span,
                "match arm result",
            )?;
        }
    }
    Ok(if valid { common } else { None })
}
