//! Exact punctuation omitted by the existing v4 DTO, authenticated from typed owner contexts.

use super::super::identifiers;
use crate::v4::{RawExpressionKind as E, RawStatementKind, SourceUnit};
use zryna_source::UntrustedSpan;

mod weak_upgrade;

type Ranges = Vec<(usize, usize)>;

pub(super) fn derive(
    file: &SourceUnit,
    source: &str,
    ranges: &mut Ranges,
) -> Result<(), &'static str> {
    for function in file.functions() {
        let types = file.type_syntax();
        let ty = |id: u32| {
            types.get(usize::try_from(id).map_err(|_| "invalid type")?).ok_or("invalid type")
        };
        let mut previous = function.name.span.end;
        let mut punctuation = b"(".as_slice();
        for parameter in &function.parameters {
            gap(source, previous, parameter.name.span.start, punctuation, false, ranges)?;
            gap(
                source,
                parameter.name.span.end,
                ty(parameter.type_syntax)?.span.start,
                b":",
                false,
                ranges,
            )?;
            previous = parameter.span.end;
            punctuation = b",";
        }
        if function.parameters.is_empty() {
            gap(source, previous, ty(function.result_type)?.span.start, b"():", false, ranges)?;
        } else {
            gap(source, previous, ty(function.result_type)?.span.start, b"):", true, ranges)?;
        }
        for statement in &function.body.statements {
            if let RawStatementKind::LocalDeclaration { name, type_syntax, .. } = &statement.kind {
                gap(source, name.span.end, ty(*type_syntax)?.span.start, b":", false, ranges)?;
            }
            if matches!(statement.kind, RawStatementKind::WeakUpgrade { .. }) {
                weak_upgrade::derive(source, function, statement, ranges)?;
            }
        }
        let expression = |id: u32| {
            function
                .body
                .expressions
                .get(usize::try_from(id).map_err(|_| "invalid expression")?)
                .map(|expression| expression.span)
                .ok_or("invalid expression")
        };
        for node in &function.body.expressions {
            match &node.kind {
                E::Call { open_paren_span, arguments, close_paren_span, .. } => {
                    let spans = arguments
                        .iter()
                        .map(|id| expression(*id))
                        .collect::<Result<Vec<_>, _>>()?;
                    list(source, open_paren_span.end, &spans, close_paren_span.start, ranges)?;
                }
                E::VecConstruction { open_bracket_span, elements, close_bracket_span, .. }
                | E::FixedArrayConstruction {
                    open_bracket_span,
                    elements,
                    close_bracket_span,
                    ..
                } => {
                    let spans =
                        elements.iter().map(|id| expression(*id)).collect::<Result<Vec<_>, _>>()?;
                    list(source, open_bracket_span.end, &spans, close_bracket_span.start, ranges)?;
                }
                E::StructConstruction { open_brace_span, fields, close_brace_span, .. } => {
                    let spans = fields.iter().map(|field| field.span).collect::<Vec<_>>();
                    list(source, open_brace_span.end, &spans, close_brace_span.start, ranges)?;
                }
                E::Match { scrutinee, open_brace_span, arms, close_brace_span, .. } => {
                    gap(
                        source,
                        expression(*scrutinee)?.end,
                        open_brace_span.start,
                        b",",
                        false,
                        ranges,
                    )?;
                    let spans = arms.iter().map(|arm| arm.span).collect::<Vec<_>>();
                    list(source, open_brace_span.end, &spans, close_brace_span.start, ranges)?;
                    for arm in arms {
                        match_arm(source, arm, ranges)?;
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn list(
    source: &str,
    start: u32,
    spans: &[UntrustedSpan],
    end: u32,
    ranges: &mut Ranges,
) -> Result<(), &'static str> {
    let mut previous = start;
    for (index, span) in spans.iter().enumerate() {
        gap(source, previous, span.start, if index == 0 { b"" } else { b"," }, false, ranges)?;
        previous = span.end;
    }
    gap(source, previous, end, b"", !spans.is_empty(), ranges)
}

fn match_arm(
    source: &str,
    arm: &crate::v4::RawMatchArm,
    ranges: &mut Ranges,
) -> Result<(), &'static str> {
    let start = usize::try_from(arm.span.start).map_err(|_| "invalid match arm")?;
    let quote = *source.as_bytes().get(start).ok_or("invalid match arm")?;
    if quote != b'"' || arm.type_name.span.start != arm.span.start + 1 {
        return Err("match arm label must be source quoted");
    }
    gap(source, arm.span.start, arm.type_name.span.start, &[quote], false, ranges)?;
    let end = arm.variant.span.end;
    gap(source, end, end + 1, &[quote], false, ranges)?;
    if let Some(binding) = &arm.binding {
        gap(source, end + 1, binding.span.start, b":(", false, ranges)?;
        gap(source, binding.span.end, arm.arrow_span.start, b")", true, ranges)?;
    } else {
        gap(source, end + 1, arm.arrow_span.start, b":()", false, ranges)?;
    }
    Ok(())
}

pub(super) fn gap(
    source: &str,
    start: u32,
    end: u32,
    punctuation: &[u8],
    optional_comma: bool,
    ranges: &mut Ranges,
) -> Result<(), &'static str> {
    let start = usize::try_from(start).map_err(|_| "invalid punctuation gap")?;
    let end = usize::try_from(end).map_err(|_| "invalid punctuation gap")?;
    let bytes = source.as_bytes().get(start..end).ok_or("invalid punctuation gap")?;
    let mut cursor = 0;
    identifiers::trivia(bytes, &mut cursor)?;
    if optional_comma && bytes.get(cursor) == Some(&b',') {
        cursor += 1;
    }
    for token in punctuation {
        identifiers::trivia(bytes, &mut cursor)?;
        if bytes.get(cursor) != Some(token) {
            return Err("unrepresented source punctuation differs");
        }
        cursor += 1;
    }
    identifiers::trivia(bytes, &mut cursor)?;
    if cursor != bytes.len() {
        return Err("unrepresented source syntax in punctuation gap");
    }
    ranges.push((start, end));
    Ok(())
}
