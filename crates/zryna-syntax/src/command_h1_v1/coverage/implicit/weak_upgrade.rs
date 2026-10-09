//! Existing v4 weak-upgrade callback delimiters omitted from its syntax DTO.

use super::{Ranges, gap};
use crate::v4::{RawFunctionSyntax, RawStatementKind, RawStatementSyntax};

pub(super) fn derive(
    source: &str,
    function: &RawFunctionSyntax,
    statement: &RawStatementSyntax,
    ranges: &mut Ranges,
) -> Result<(), &'static str> {
    let RawStatementKind::WeakUpgrade {
        keyword_span,
        weak,
        as_span,
        binding,
        success_block,
        else_span,
        failure_block,
    } = &statement.kind
    else {
        return Err("invalid weak-upgrade context");
    };
    let body = &function.body;
    let weak = usize::try_from(*weak)
        .ok()
        .and_then(|id| body.expressions.get(id))
        .ok_or("invalid weak-upgrade expression")?
        .span;
    let block = |id: u32| {
        usize::try_from(id)
            .ok()
            .and_then(|id| body.blocks.get(id))
            .map(|block| block.span)
            .ok_or("invalid weak-upgrade block")
    };
    let success = block(*success_block)?;
    let failure = block(*failure_block)?;
    gap(source, keyword_span.end, weak.start, b"(", false, ranges)?;
    gap(source, weak.end, binding.span.start, b",(", false, ranges)?;
    gap(source, binding.span.end, as_span.start, b")", true, ranges)?;
    gap(source, as_span.end, success.start, b"", false, ranges)?;
    gap(source, success.end, else_span.start, b",()", false, ranges)?;
    gap(source, else_span.end, failure.start, b"", false, ranges)?;
    gap(source, failure.end, statement.span.end, b");", true, ranges)
}
