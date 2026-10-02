//! Expressions for the straight-line protocol-v3 candidate.

use zryna_source::UntrustedSpan;
use zryna_syntax::v3 as syntax;

use crate::native_lexer::{Keyword, Token, TokenKind};

use super::{FileParser, ParseError, function_error_at, raw, resource};

mod operators;

impl FileParser<'_> {
    fn negation(
        &self,
        expressions: &mut Vec<syntax::RawExpressionSyntax>,
        previous_expressions: usize,
        operator: Token,
        operand: u32,
        operand_depth: u32,
        end: u32,
    ) -> Result<(u32, u32), ParseError> {
        let index = push_expression(
            expressions,
            previous_expressions,
            syntax::RawExpressionSyntax {
                span: UntrustedSpan { file: self.file, start: operator.span().start(), end },
                kind: syntax::RawExpressionKind::Negation { operator_span: raw(operator), operand },
            },
        )?;
        Ok((index, operand_depth + 1))
    }

    fn numeric_negation(
        &mut self,
        expressions: &mut Vec<syntax::RawExpressionSyntax>,
        previous_expressions: usize,
        operator: Token,
    ) -> Result<(u32, u32), ParseError> {
        let digits = self.function_take(TokenKind::DecimalInteger)?;
        let digits_spelling = self.spelling(digits);
        if digits_spelling.len() > syntax::MAX_LITERAL_BYTES
            || (digits_spelling != "0" && digits_spelling.starts_with('0'))
        {
            return Err(function_error_at(operator, "noncanonical integer literal"));
        }
        if operator.span().end() == digits.span().start()
            && digits_spelling != "0"
            && digits_spelling.len() < syntax::MAX_LITERAL_BYTES
        {
            let spelling =
                &self.text[operator.span().start() as usize..digits.span().end() as usize];
            return push_expression(
                expressions,
                previous_expressions,
                syntax::RawExpressionSyntax {
                    span: UntrustedSpan {
                        file: self.file,
                        start: operator.span().start(),
                        end: digits.span().end(),
                    },
                    kind: syntax::RawExpressionKind::I32Literal { spelling: spelling.to_owned() },
                },
            )
            .map(|index| (index, 1));
        }
        let operand = push_expression(
            expressions,
            previous_expressions,
            syntax::RawExpressionSyntax {
                span: raw(digits),
                kind: syntax::RawExpressionKind::I32Literal {
                    spelling: digits_spelling.to_owned(),
                },
            },
        )?;
        self.negation(expressions, previous_expressions, operator, operand, 1, digits.span().end())
    }

    pub(super) fn expression(
        &mut self,
        expressions: &mut Vec<syntax::RawExpressionSyntax>,
        previous_expressions: usize,
        block_depth: u32,
    ) -> Result<u32, ParseError> {
        self.current().ok_or_else(|| self.function_error_here("missing expression"))?;
        if let Some(diagnostic) = crate::native_parser::depth::source_diagnostic(
            self.sources,
            self.text,
            &self.tokens[self.position..],
            block_depth,
            3,
            crate::native_parser::depth::ExpressionBudgets {
                function: expressions.len(),
                project: previous_expressions + expressions.len(),
                aggregate: 0,
            },
        ) {
            return Err(ParseError { diagnostic });
        }
        let (index, depth) = operators::parse(self, expressions, previous_expressions, 1)?;
        if depth + block_depth > syntax::MAX_NESTING_DEPTH {
            return Err(ParseError {
                diagnostic: crate::native_parser::depth::v3_diagnostic(
                    self.sources,
                    expressions,
                    index,
                    block_depth,
                )
                .expect("overflowing expression depth"),
            });
        }
        Ok(index)
    }

    fn direct_call(
        &mut self,
        expressions: &mut Vec<syntax::RawExpressionSyntax>,
        previous_expressions: usize,
        call_nesting: u32,
    ) -> Result<(u32, u32), ParseError> {
        let callee = self.function_identifier()?;
        let open = self.function_take(TokenKind::OpenParen)?;
        if super::super::super::collections::bounds(&self.tokens, self.position - 1)
            .is_some_and(|(_, count)| count > syntax::MAX_PARAMETERS_PER_FUNCTION)
        {
            return Err(resource("call exceeds the argument limit"));
        }
        let mut arguments = Vec::new();
        let mut argument_depth = 0;
        if self.current().is_some_and(|token| token.kind() != TokenKind::CloseParen) {
            loop {
                if arguments.len() >= syntax::MAX_PARAMETERS_PER_FUNCTION {
                    return Err(resource("call exceeds the argument limit"));
                }
                if call_nesting >= syntax::MAX_NESTING_DEPTH {
                    return Err(resource("expression nesting exceeds protocol-v3 limit"));
                }
                let (argument, depth) =
                    operators::parse(self, expressions, previous_expressions, call_nesting + 1)?;
                arguments.push(argument);
                argument_depth = argument_depth.max(depth);
                if self.maybe(TokenKind::Comma).is_none()
                    || self.current().is_some_and(|token| token.kind() == TokenKind::CloseParen)
                {
                    break;
                }
            }
        }
        let close = self.function_take(TokenKind::CloseParen)?;
        let index = push_expression(
            expressions,
            previous_expressions,
            syntax::RawExpressionSyntax {
                span: UntrustedSpan {
                    file: self.file,
                    start: callee.span.start,
                    end: close.span().end(),
                },
                kind: syntax::RawExpressionKind::Call {
                    callee,
                    open_paren_span: raw(open),
                    arguments,
                    close_paren_span: raw(close),
                },
            },
        )?;
        Ok((index, argument_depth + 1))
    }

    fn operand(
        &mut self,
        expressions: &mut Vec<syntax::RawExpressionSyntax>,
        previous_expressions: usize,
        call_nesting: u32,
    ) -> Result<(u32, u32), ParseError> {
        let mut minuses = Vec::new();
        while let Some(token) = self.current().filter(|token| token.kind() == TokenKind::Minus) {
            if minuses
                .last()
                .is_some_and(|previous: &Token| previous.span().end() == token.span().start())
            {
                return Err(function_error_at(token, "unsupported decrement"));
            }
            minuses.push(token);
            self.position += 1;
        }
        let token = self.current().ok_or_else(|| self.function_error_here("missing expression"))?;
        let (mut index, mut depth) =
            if token.kind() == TokenKind::DecimalInteger && !minuses.is_empty() {
                self.numeric_negation(
                    expressions,
                    previous_expressions,
                    minuses.pop().expect("one numeric operator"),
                )?
            } else if token.kind() == TokenKind::Identifier
                && self
                    .tokens
                    .get(self.position + 1)
                    .is_some_and(|next| next.kind() == TokenKind::OpenParen)
            {
                self.direct_call(expressions, previous_expressions, call_nesting)?
            } else {
                let kind = match token.kind() {
                    TokenKind::Identifier => {
                        syntax::RawExpressionKind::Reference { name: self.function_identifier()? }
                    }
                    TokenKind::Keyword(Keyword::True | Keyword::False) => {
                        self.position += 1;
                        syntax::RawExpressionKind::BoolLiteral {
                            value: token.kind() == TokenKind::Keyword(Keyword::True),
                        }
                    }
                    TokenKind::DecimalInteger => {
                        let spelling = self.spelling(token);
                        if spelling.len() > syntax::MAX_LITERAL_BYTES
                            || (spelling != "0" && spelling.starts_with('0'))
                        {
                            return Err(self.function_error_here("noncanonical integer literal"));
                        }
                        let spelling = spelling.to_owned();
                        self.position += 1;
                        syntax::RawExpressionKind::I32Literal { spelling }
                    }
                    _ => return Err(self.function_error_here("unsupported expression")),
                };
                let index = push_expression(
                    expressions,
                    previous_expressions,
                    syntax::RawExpressionSyntax { span: raw(token), kind },
                )?;
                (index, 1)
            };
        for operator in minuses.into_iter().rev() {
            let end = expressions[index as usize].span.end;
            (index, depth) =
                self.negation(expressions, previous_expressions, operator, index, depth, end)?;
        }
        Ok((index, depth))
    }
}

fn push_expression(
    expressions: &mut Vec<syntax::RawExpressionSyntax>,
    previous_expressions: usize,
    expression: syntax::RawExpressionSyntax,
) -> Result<u32, ParseError> {
    if expressions.len() >= syntax::MAX_EXPRESSIONS_PER_FUNCTION
        || previous_expressions + expressions.len() >= syntax::MAX_EXPRESSIONS_PER_PROJECT
    {
        return Err(resource(if expressions.len() >= syntax::MAX_EXPRESSIONS_PER_FUNCTION {
            "function exceeds the expression limit"
        } else {
            "project exceeds the expression limit"
        }));
    }
    let index = u32::try_from(expressions.len())
        .map_err(|_| resource("expression inventory exceeds protocol-v3 limit"))?;
    expressions.push(expression);
    Ok(index)
}
