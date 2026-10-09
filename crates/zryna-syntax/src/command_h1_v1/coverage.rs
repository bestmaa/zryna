//! Complete file coverage from authenticated leaves, never from a wide provider parent span.

use serde_json::Value;

use super::identifiers;
use crate::v4::{RawExpressionKind, SourceUnit};

mod aliases;
mod implicit;
mod leaves;

pub(super) fn verify(file: &SourceUnit, source: &str) -> Result<(), &'static str> {
    // The verified v4 unit already obeys its response, arena and source bounds. The JSON value
    // is a read-only traversal of that immutable DTO, not an independently trusted classifier.
    let dto =
        serde_json::to_value(file).map_err(|_| "command source coverage could not be derived")?;
    let mut ranges = Vec::new();
    let mut pending = vec![&dto];
    while let Some(value) = pending.pop() {
        match value {
            Value::Array(items) => pending.extend(items),
            Value::Object(fields) => {
                if fields.get("text").is_some_and(Value::is_string)
                    && let Some(span) = fields.get("span")
                {
                    let span = leaves::range(span)?;
                    leaves::validate(source, span, "identifier")?;
                    ranges.push(span);
                }
                for (key, child) in fields {
                    if key.ends_with("_span") && !child.is_null() {
                        let span = leaves::range(child)?;
                        leaves::validate(source, span, key)?;
                        ranges.push(span);
                    } else if key != "span" {
                        pending.push(child);
                    }
                }
            }
            _ => {}
        }
    }
    for function in file.functions() {
        for expression in &function.body.expressions {
            let role = match expression.kind {
                RawExpressionKind::BoolLiteral { .. } => Some("bool-literal"),
                RawExpressionKind::I32Literal { .. } => Some("i32-literal"),
                RawExpressionKind::StringLiteral { .. } => Some("string-literal"),
                _ => None,
            };
            if let Some(role) = role {
                let range = (
                    usize::try_from(expression.span.start).map_err(|_| "invalid leaf")?,
                    usize::try_from(expression.span.end).map_err(|_| "invalid leaf")?,
                );
                leaves::validate(source, range, role)?;
                ranges.push(range);
            }
        }
    }
    aliases::verify(file, &mut ranges)?;
    implicit::derive(file, source, &mut ranges)?;
    ranges.sort_unstable();
    let mut cursor = 0;
    for (start, end) in ranges {
        if end > source.len()
            || start > end
            || !source.is_char_boundary(start)
            || !source.is_char_boundary(end)
        {
            return Err("invalid command source coverage leaf");
        }
        if start == end {
            continue;
        }
        if start < cursor {
            return Err("overlapping implicit command source context");
        }
        if start > cursor {
            only_trivia(source, cursor, start)?;
        }
        cursor = cursor.max(end);
    }
    only_trivia(source, cursor, source.len())
}

fn only_trivia(source: &str, start: usize, end: usize) -> Result<(), &'static str> {
    let gap = source.as_bytes().get(start..end).ok_or("invalid coverage gap")?;
    let mut cursor = 0;
    identifiers::trivia(gap, &mut cursor)?;
    if cursor == gap.len() {
        Ok(())
    } else {
        Err("command provider omitted nontrivia source syntax")
    }
}
