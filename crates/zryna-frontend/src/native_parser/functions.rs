//! Function signatures and return bodies for protocol v2.

use super::{AnnotationContext, FileParser, ParseError, expression, has_line_break, raw, recovery};
use crate::native_lexer::{Keyword, TokenKind};
use zryna_source::UntrustedSpan;
use zryna_syntax::v2 as syntax;

impl FileParser<'_> {
    pub(super) fn function(
        &mut self,
        index: usize,
    ) -> Result<syntax::RawFunctionSyntax, ParseError> {
        let (export, keyword) = self.function_prefix(index)?;
        let name = self.identifier()?;
        self.recover_type_parameters(index)?;
        let (parameters, parameter_end) = self.parameters()?;
        let result_type = self.annotation(parameter_end, AnnotationContext::Result(index))?;
        let open = self.take(TokenKind::OpenBrace)?;
        let mut statements = Vec::new();
        let mut expressions = Vec::new();
        while self.current().is_some_and(|token| token.kind() != TokenKind::CloseBrace) {
            if self.statement_count >= syntax::MAX_STATEMENTS_PER_FUNCTION {
                return Err(super::resource("function exceeds the statement limit"));
            }
            self.statement_count += 1;
            if self.previous_statements + self.statement_count > syntax::MAX_STATEMENTS_PER_PROJECT
            {
                return Err(super::resource("project exceeds the statement limit"));
            }
            let checkpoint = self.position;
            let expression_checkpoint = expressions.len();
            match self.return_statement(&mut expressions) {
                Ok(Some(statement)) => statements.push(statement),
                Ok(None) => expressions.truncate(expression_checkpoint),
                Err(error) if self.recovering && error.diagnostic().code() == "ZRYNA-F2002" => {
                    self.retain_error(&error);
                    if let Some(following) = error.following {
                        self.statement_count += 1;
                        if self.statement_count > syntax::MAX_STATEMENTS_PER_FUNCTION {
                            return Err(super::resource("function exceeds the statement limit"));
                        }
                        if self.previous_statements + self.statement_count
                            > syntax::MAX_STATEMENTS_PER_PROJECT
                        {
                            return Err(super::resource("project exceeds the statement limit"));
                        }
                        self.retain_diagnostic(*following);
                    }
                    expressions.truncate(expression_checkpoint);
                    recovery::skip_statement(self, checkpoint)?;
                }
                Err(error) => return Err(error),
            }
        }
        let close = self.take(TokenKind::CloseBrace)?;
        Ok(syntax::RawFunctionSyntax {
            span: UntrustedSpan {
                file: self.file,
                start: export.span().start(),
                end: close.span().end(),
            },
            export_span: raw(export),
            function_span: raw(keyword),
            name,
            parameters,
            result_type,
            body: syntax::RawFunctionBodySyntax {
                span: UntrustedSpan {
                    file: self.file,
                    start: open.span().start(),
                    end: close.span().end(),
                },
                statements,
                expressions,
            },
        })
    }

    fn return_statement(
        &mut self,
        expressions: &mut Vec<syntax::RawExpressionSyntax>,
    ) -> Result<Option<syntax::RawStatementSyntax>, ParseError> {
        if self.recovering && self.recover_statement() {
            return Ok(None);
        }
        let keyword = self.take(TokenKind::Keyword(Keyword::Return))?;
        if self.current().is_some_and(|next| {
            has_line_break(self.text, keyword.span().end(), next.span().start())
        }) {
            let mut error = self.error_between(
                keyword,
                keyword,
                "statement uses unsupported syntax 'ReturnStatement'",
            );
            error.following = recovery::newline_expression_statement(self).map(Box::new);
            return Err(error);
        }
        let expression_start = self.position;
        let value = if self.recovering {
            expression::recovering_addition(self, expressions)?
        } else {
            Some(expression::addition(self, expressions)?)
        };
        if self.position == expression_start {
            return Err(self.error_here("ZRYNA-F2002", "return value is missing"));
        }
        let expression_end = self.tokens[self.position - 1].span().end();
        let statement_end = if let Some(semicolon) = self.maybe(TokenKind::Semicolon) {
            semicolon.span().end()
        } else if self.current().is_some_and(|next| {
            next.kind() == TokenKind::CloseBrace
                || (next.kind() == TokenKind::Keyword(Keyword::Return)
                    && has_line_break(self.text, expression_end, next.span().start()))
        }) {
            expression_end
        } else {
            return Err(self.error_here("ZRYNA-F2002", "missing return statement terminator"));
        };
        Ok(value.map(|value| syntax::RawStatementSyntax {
            span: UntrustedSpan {
                file: self.file,
                start: keyword.span().start(),
                end: statement_end,
            },
            kind: syntax::RawStatementKind::Return { keyword_span: raw(keyword), value },
        }))
    }

    fn parameters(&mut self) -> Result<(Vec<syntax::RawParameterSyntax>, u32), ParseError> {
        let open_paren = self.take(TokenKind::OpenParen)?;
        let mut parameter_end = open_paren.span().end();
        let mut parameters = Vec::new();
        if self.current().is_some_and(|token| token.kind() != TokenKind::CloseParen) {
            loop {
                if parameters.len() >= syntax::MAX_PARAMETERS_PER_FUNCTION {
                    return Err(super::resource("function exceeds the parameter limit"));
                }
                self.parameter_count += 1;
                if self.previous_parameters + self.parameter_count
                    > syntax::MAX_PARAMETERS_PER_PROJECT
                {
                    return Err(super::resource("project exceeds the parameter limit"));
                }
                let name = self.identifier()?;
                let type_syntax =
                    self.annotation(name.span.end, AnnotationContext::Parameter(parameters.len()))?;
                self.recover_parameter_initializer(parameters.len())?;
                parameter_end = type_syntax.span.end;
                parameters.push(syntax::RawParameterSyntax {
                    span: UntrustedSpan {
                        file: self.file,
                        start: name.span.start,
                        end: type_syntax.span.end,
                    },
                    name,
                    type_syntax,
                });
                let Some(comma) = self.maybe(TokenKind::Comma) else {
                    break;
                };
                parameter_end = comma.span().end();
                if self.current().is_some_and(|token| token.kind() == TokenKind::CloseParen) {
                    break;
                }
            }
        }
        self.take(TokenKind::CloseParen)?;
        Ok((parameters, parameter_end))
    }
}
