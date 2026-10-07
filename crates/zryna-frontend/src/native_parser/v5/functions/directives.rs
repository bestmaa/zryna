//! Directives that change identifier grammar are outside this source syntax slice.

use super::super::{FileParser, ParseError, syntax, unsupported};

impl FileParser<'_> {
    pub(super) fn reject_directives(
        body: &syntax::RawFunctionBodySyntax,
    ) -> Result<(), ParseError> {
        for id in &body.blocks[body.root_block as usize].statements {
            let syntax::RawStatementKind::ExpressionStatement { expression, .. } =
                body.statements[*id as usize].kind
            else {
                break;
            };
            let syntax::RawExpressionKind::StringLiteral { spelling } =
                &body.expressions[expression as usize].kind
            else {
                break;
            };
            if matches!(spelling.as_str(), "\"use strict\"" | "'use strict'") {
                return Err(unsupported(None, "strict directives are excluded from v5 syntax"));
            }
        }
        Ok(())
    }
}
