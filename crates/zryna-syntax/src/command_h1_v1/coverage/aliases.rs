//! The only duplicate source leaf is a v4 shorthand field and its exact reference expression.

use crate::v4::{RawExpressionKind, RawFieldInitializerKind, SourceUnit};
use std::collections::BTreeSet;

pub(super) fn verify(
    file: &SourceUnit,
    ranges: &mut Vec<(usize, usize)>,
) -> Result<(), &'static str> {
    let mut aliases = BTreeSet::new();
    for function in file.functions() {
        for expression in &function.body.expressions {
            if let RawExpressionKind::StructConstruction { fields, .. } = &expression.kind {
                for field in fields {
                    if let RawFieldInitializerKind::Shorthand { name, value } = &field.kind {
                        let value = usize::try_from(*value)
                            .ok()
                            .and_then(|index| function.body.expressions.get(index))
                            .ok_or("invalid shorthand")?;
                        let RawExpressionKind::Reference { name: reference } = &value.kind else {
                            return Err("shorthand is not an exact reference");
                        };
                        if name != reference || value.span != name.span {
                            return Err("shorthand does not have one exact source leaf alias");
                        }
                        aliases.insert((
                            usize::try_from(name.span.start).map_err(|_| "invalid alias")?,
                            usize::try_from(name.span.end).map_err(|_| "invalid alias")?,
                        ));
                    }
                }
            }
        }
    }
    ranges.sort_unstable();
    let mut previous = None;
    for &range in ranges.iter() {
        if let Some(prior) = previous {
            if range == prior {
                if !aliases.remove(&range) {
                    return Err("unapproved duplicate source leaf");
                }
            } else if range.0 < prior.1 {
                return Err("overlapping command source leaves");
            }
        }
        previous = Some(range);
    }
    ranges.dedup();
    Ok(())
}
