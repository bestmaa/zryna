use super::super::{error, outcome_shape, raw};
use std::collections::BTreeSet;
use zryna_diagnostics::Diagnostic;
use zryna_layout::VerifiedLayouts;
use zryna_source::SourceMap;
use zryna_syntax::v4::{RawExpressionKind, RawFunctionSyntax};

pub(super) fn has_source_match(function: &RawFunctionSyntax) -> bool {
    function.body.expressions.iter().any(|expression| {
        matches!(&expression.kind, RawExpressionKind::Match { arms, .. }
            if arms.iter().any(|arm| arm.type_name.text == "EnvLookupV1"))
    })
}

pub(super) fn verify(
    function: &raw::Function,
    source: &RawFunctionSyntax,
    sources: &SourceMap,
    layouts: &VerifiedLayouts,
    declaration: u32,
) -> Result<(), Vec<Diagnostic>> {
    let mut expected = BTreeSet::new();
    for expression in &source.body.expressions {
        if let RawExpressionKind::Match { arms, .. } = &expression.kind
            && arms.iter().any(|arm| arm.type_name.text == "EnvLookupV1")
        {
            let variants =
                arms.iter().map(|arm| arm.variant.text.as_str()).collect::<BTreeSet<_>>();
            if arms.len() != 2
                || arms.iter().any(|arm| arm.type_name.text != "EnvLookupV1")
                || variants != BTreeSet::from(["Found", "Missing"])
                || arms.iter().any(|arm| (arm.variant.text == "Found") != arm.binding.is_some())
            {
                return Err(vec![error(
                    "environment source match must exhaust exactly Found(String) and Missing",
                )]);
            }
            let span = sources
                .verify_span(expression.span)
                .map_err(|_| vec![error("source outcome match is stale")])?;
            expected.insert((span.start(), span.end()));
        }
    }
    let mut actual = BTreeSet::new();
    for terminator in function.blocks.iter().flat_map(|block| &block.terminators) {
        if let raw::Terminator::EnumMatch { place, .. } = &terminator.kind {
            let is_outcome = usize::try_from(place.0)
                .ok()
                .and_then(|id| function.places.get(id))
                .is_some_and(|place| outcome_shape(layouts, place.ty) == Some((0, declaration)));
            if is_outcome && !actual.insert((terminator.span.start(), terminator.span.end())) {
                return Err(vec![error("command outcome match was duplicated")]);
            }
        }
    }
    if actual != expected {
        return Err(vec![error(
            "command IR omitted or invented an exhaustive source outcome match",
        )]);
    }
    Ok(())
}
