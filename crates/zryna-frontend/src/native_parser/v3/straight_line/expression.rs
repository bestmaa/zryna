//! Expressions for the straight-line protocol-v3 candidate.

use zryna_source::UntrustedSpan;
use zryna_syntax::v3 as syntax;

use crate::native_lexer::{Keyword, Token, TokenKind};

use super::{FileParser, ParseError, function_error_at, raw, resource};

impl FileParser<'_> {
    fn negation(
        &self,
        expressions: &mut Vec<syntax::RawExpressionSyntax>,
        previous_expressions: usize,
        operator: Token,
        operand: u32,
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
        Ok((index, 2))
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
        self.negation(expressions, previous_expressions, operator, operand, digits.span().end())
    }

    pub(super) fn addition(
        &mut self,
        expressions: &mut Vec<syntax::RawExpressionSyntax>,
        previous_expressions: usize,
    ) -> Result<u32, ParseError> {
        let (mut left, mut depth) = self.atom(expressions, previous_expressions)?;
        while let Some(operator) = self.maybe(TokenKind::Plus) {
            if depth >= syntax::MAX_NESTING_DEPTH {
                return Err(resource("expression nesting exceeds protocol-v3 limit"));
            }
            let (right, right_depth) = self.atom(expressions, previous_expressions)?;
            let next_depth = depth.max(right_depth) + 1;
            if next_depth > syntax::MAX_NESTING_DEPTH {
                return Err(resource("expression nesting exceeds protocol-v3 limit"));
            }
            let span = UntrustedSpan {
                file: self.file,
                start: expressions[left as usize].span.start,
                end: expressions[right as usize].span.end,
            };
            left = push_expression(
                expressions,
                previous_expressions,
                syntax::RawExpressionSyntax {
                    span,
                    kind: syntax::RawExpressionKind::Addition {
                        operator_span: raw(operator),
                        lhs: left,
                        rhs: right,
                    },
                },
            )?;
            depth = next_depth;
        }
        Ok(left)
    }

    fn zero_argument_call(
        &mut self,
        expressions: &mut Vec<syntax::RawExpressionSyntax>,
        previous_expressions: usize,
    ) -> Result<u32, ParseError> {
        let callee = self.function_identifier()?;
        let open = self.function_take(TokenKind::OpenParen)?;
        let close = self.function_take(TokenKind::CloseParen)?;
        push_expression(
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
                    arguments: Vec::new(),
                    close_paren_span: raw(close),
                },
            },
        )
    }

    fn atom(
        &mut self,
        expressions: &mut Vec<syntax::RawExpressionSyntax>,
        previous_expressions: usize,
    ) -> Result<(u32, u32), ParseError> {
        let token = self.current().ok_or_else(|| self.function_error_here("missing expression"))?;
        if token.kind() == TokenKind::Minus {
            self.position += 1;
            if self.current().is_some_and(|next| next.kind() == TokenKind::Identifier)
                && !self
                    .tokens
                    .get(self.position + 1)
                    .is_some_and(|next| next.kind() == TokenKind::OpenParen)
            {
                let name = self.function_identifier()?;
                let operand_span = name.span;
                let operand = push_expression(
                    expressions,
                    previous_expressions,
                    syntax::RawExpressionSyntax {
                        span: operand_span,
                        kind: syntax::RawExpressionKind::Reference { name },
                    },
                )?;
                return self.negation(
                    expressions,
                    previous_expressions,
                    token,
                    operand,
                    operand_span.end,
                );
            }
            return self.numeric_negation(expressions, previous_expressions, token);
        }
        if token.kind() == TokenKind::Identifier
            && self
                .tokens
                .get(self.position + 1)
                .is_some_and(|next| next.kind() == TokenKind::OpenParen)
        {
            return self
                .zero_argument_call(expressions, previous_expressions)
                .map(|index| (index, 1));
        }
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
        push_expression(
            expressions,
            previous_expressions,
            syntax::RawExpressionSyntax { span: raw(token), kind },
        )
        .map(|index| (index, 1))
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
        return Err(resource("expression inventory exceeds protocol-v3 limit"));
    }
    let index = u32::try_from(expressions.len())
        .map_err(|_| resource("expression inventory exceeds protocol-v3 limit"))?;
    expressions.push(expression);
    Ok(index)
}
