//! Scalar expressions and canonical postorder expression inventory.

use super::super::syntax;
use zryna_source::UntrustedSpan;

use crate::native_lexer::{Keyword, Token, TokenKind};

use super::super::{FileParser, ParseError, raw, resource, unsupported};
use super::Body;

impl FileParser<'_> {
    pub(super) fn expression(
        &mut self,
        body: &mut Body,
        block_depth: u32,
    ) -> Result<u32, ParseError> {
        self.current().ok_or_else(|| unsupported(None, "missing expression"))?;
        let (id, depth) = self.binary_expression(body, 1)?;
        if depth + block_depth > syntax::MAX_NESTING_DEPTH {
            return Err(resource("expression exceeds the nesting limit"));
        }
        Ok(id)
    }

    pub(super) fn binary_expression(
        &mut self,
        body: &mut Body,
        nesting: u32,
    ) -> Result<(u32, u32), ParseError> {
        let mut values = vec![self.operand(body, nesting)?];
        let mut operators: Vec<(Token, u8)> = Vec::new();
        while let Some(token) = self.current() {
            let Some(precedence) = precedence(token.kind()) else { break };
            self.position += 1;
            if matches!(token.kind(), TokenKind::Plus | TokenKind::Minus)
                && self.current().is_some_and(|next| {
                    next.kind() == token.kind() && token.span().end() == next.span().start()
                })
            {
                return Err(unsupported(Some(token), "unsupported increment or decrement"));
            }
            while operators.last().is_some_and(|(_, current)| *current >= precedence) {
                let (operator, _) = operators.pop().expect("pending binary operator");
                self.reduce(body, &mut values, operator)?;
            }
            operators.push((token, precedence));
            values.push(self.operand(body, nesting)?);
        }
        while let Some((operator, _)) = operators.pop() {
            self.reduce(body, &mut values, operator)?;
        }
        Ok(values.pop().expect("expression root"))
    }

    fn reduce(
        &self,
        body: &mut Body,
        values: &mut Vec<(u32, u32)>,
        operator: Token,
    ) -> Result<(), ParseError> {
        let (rhs, rhs_depth) = values.pop().expect("binary right operand");
        let (lhs, lhs_depth) = values.pop().expect("binary left operand");
        let depth = lhs_depth.max(rhs_depth) + 1;
        if depth > syntax::MAX_NESTING_DEPTH {
            return Err(resource("expression exceeds the nesting limit"));
        }
        let span = UntrustedSpan {
            file: self.file,
            start: body.expressions[lhs as usize].span.start,
            end: body.expressions[rhs as usize].span.end,
        };
        let operator_span = raw(operator);
        let kind = match operator.kind() {
            TokenKind::Plus => syntax::RawExpressionKind::Addition { operator_span, lhs, rhs },
            TokenKind::Minus => syntax::RawExpressionKind::Subtraction { operator_span, lhs, rhs },
            TokenKind::Asterisk => {
                syntax::RawExpressionKind::Multiplication { operator_span, lhs, rhs }
            }
            TokenKind::StrictEqual => syntax::RawExpressionKind::Equal { operator_span, lhs, rhs },
            TokenKind::StrictNotEqual => {
                syntax::RawExpressionKind::NotEqual { operator_span, lhs, rhs }
            }
            TokenKind::LessThan => syntax::RawExpressionKind::LessThan { operator_span, lhs, rhs },
            TokenKind::LessEqual => {
                syntax::RawExpressionKind::LessEqual { operator_span, lhs, rhs }
            }
            TokenKind::GreaterThan => {
                syntax::RawExpressionKind::GreaterThan { operator_span, lhs, rhs }
            }
            TokenKind::GreaterEqual => {
                syntax::RawExpressionKind::GreaterEqual { operator_span, lhs, rhs }
            }
            _ => unreachable!("binary operator stack"),
        };
        let id = self.push_expression(body, syntax::RawExpressionSyntax { span, kind })?;
        values.push((id, depth));
        Ok(())
    }

    fn operand(&mut self, body: &mut Body, nesting: u32) -> Result<(u32, u32), ParseError> {
        if nesting > syntax::MAX_NESTING_DEPTH {
            return Err(resource("expression nesting exceeds protocol-v5 limit"));
        }
        let mut minuses = Vec::new();
        while let Some(token) = self.current().filter(|token| token.kind() == TokenKind::Minus) {
            if minuses
                .last()
                .is_some_and(|previous: &Token| previous.span().end() == token.span().start())
            {
                return Err(unsupported(Some(token), "unsupported decrement"));
            }
            minuses.push(token);
            if minuses.len() >= syntax::MAX_NESTING_DEPTH as usize {
                return Err(resource("expression nesting exceeds protocol-v5 limit"));
            }
            self.position += 1;
        }
        let first = self.current().ok_or_else(|| unsupported(None, "missing expression"))?;
        let (mut id, mut depth) = if matches!(
            first.kind(),
            TokenKind::Identifier
                | TokenKind::Keyword(
                    Keyword::As | Keyword::From | Keyword::Let | Keyword::Interface
                )
        ) && self.spelling(first) == "match"
            && self.next().is_some_and(|next| next.kind() == TokenKind::OpenParen)
        {
            self.match_expression(body, nesting)?
        } else if let Some(type_end) = self.typed_array_end() {
            self.typed_array(body, nesting, type_end)?
        } else if matches!(
            first.kind(),
            TokenKind::Identifier
                | TokenKind::Keyword(
                    Keyword::As | Keyword::From | Keyword::Let | Keyword::Interface
                )
        ) && self.next().is_some_and(|next| next.kind() == TokenKind::Dot)
            && self.call_open(self.position + 2).is_some()
        {
            self.enum_construction(body, nesting)?
        } else if matches!(
            first.kind(),
            TokenKind::Identifier
                | TokenKind::Keyword(
                    Keyword::As | Keyword::From | Keyword::Let | Keyword::Interface
                )
        ) && self.call_open(self.position).is_some_and(|open| {
            self.tokens.get(open + 1).is_some_and(|next| next.kind() == TokenKind::OpenBrace)
        }) && !matches!(
            self.spelling(first),
            "clone"
                | "borrow"
                | "borrowMut"
                | "shared"
                | "downgrade"
                | "push"
                | "match"
                | "upgradeWeak"
        ) {
            self.struct_construction(body, nesting)?
        } else if matches!(
            first.kind(),
            TokenKind::Identifier
                | TokenKind::Keyword(
                    Keyword::As | Keyword::From | Keyword::Let | Keyword::Interface
                )
        ) && self.call_open(self.position).is_some()
        {
            self.call(body, nesting)?
        } else if first.kind() == TokenKind::DecimalInteger
            && !minuses.is_empty()
            && minuses.last().is_some_and(|minus| minus.span().end() == first.span().start())
            && self.spelling(first) != "0"
        {
            let minus = minuses.pop().expect("numeric negation");
            self.numeric_negation(body, minus, first)?
        } else {
            self.simple_operand(body, first)?
        };
        (id, depth) = self.postfix(body, id, depth, nesting)?;
        for minus in minuses.into_iter().rev() {
            depth += 1;
            let span = UntrustedSpan {
                file: self.file,
                start: minus.span().start(),
                end: body.expressions[id as usize].span.end,
            };
            id = self.push_expression(
                body,
                syntax::RawExpressionSyntax {
                    span,
                    kind: syntax::RawExpressionKind::Negation {
                        operator_span: raw(minus),
                        operand: id,
                    },
                },
            )?;
        }
        Ok((id, depth))
    }

    fn numeric_negation(
        &mut self,
        body: &mut Body,
        minus: Token,
        digits: Token,
    ) -> Result<(u32, u32), ParseError> {
        let spelling =
            self.text[minus.span().start() as usize..digits.span().end() as usize].to_owned();
        if spelling.len() > 64 || spelling[1..].starts_with('0') {
            return Err(unsupported(Some(digits), "noncanonical integer literal"));
        }
        self.position += 1;
        let span = UntrustedSpan {
            file: self.file,
            start: minus.span().start(),
            end: digits.span().end(),
        };
        let id = self.push_expression(
            body,
            syntax::RawExpressionSyntax {
                span,
                kind: syntax::RawExpressionKind::I32Literal { spelling },
            },
        )?;
        Ok((id, 1))
    }

    fn simple_operand(&mut self, body: &mut Body, first: Token) -> Result<(u32, u32), ParseError> {
        let kind = match first.kind() {
            TokenKind::Identifier
            | TokenKind::Keyword(Keyword::As | Keyword::From | Keyword::Let | Keyword::Interface) => {
                syntax::RawExpressionKind::Reference {
                    name: self.name(super::super::names::Role::Runtime)?,
                }
            }
            TokenKind::Keyword(Keyword::True | Keyword::False) => {
                self.position += 1;
                syntax::RawExpressionKind::BoolLiteral {
                    value: first.kind() == TokenKind::Keyword(Keyword::True),
                }
            }
            TokenKind::DecimalInteger => {
                let spelling = self.spelling(first);
                if spelling.len() > 64 || (spelling != "0" && spelling.starts_with('0')) {
                    return Err(unsupported(Some(first), "noncanonical integer literal"));
                }
                let spelling = spelling.to_owned();
                self.position += 1;
                syntax::RawExpressionKind::I32Literal { spelling }
            }
            TokenKind::StringLiteral => {
                let spelling = self.spelling(first).to_owned();
                self.position += 1;
                syntax::RawExpressionKind::StringLiteral { spelling }
            }
            _ => return Err(unsupported(Some(first), "unsupported protocol-v5 expression")),
        };
        Ok((self.push_expression(body, syntax::RawExpressionSyntax { span: raw(first), kind })?, 1))
    }

    fn postfix(
        &mut self,
        body: &mut Body,
        mut id: u32,
        mut depth: u32,
        nesting: u32,
    ) -> Result<(u32, u32), ParseError> {
        loop {
            if let Some(dot) = self.maybe(TokenKind::Dot) {
                let field = self.identifier()?;
                let span = UntrustedSpan {
                    file: self.file,
                    start: body.expressions[id as usize].span.start,
                    end: field.span.end,
                };
                id = self.push_expression(
                    body,
                    syntax::RawExpressionSyntax {
                        span,
                        kind: syntax::RawExpressionKind::FieldAccess {
                            base: id,
                            dot_span: raw(dot),
                            field,
                        },
                    },
                )?;
                depth += 1;
            } else if let Some(open) = self.maybe(TokenKind::OpenBracket) {
                let (index, index_depth) = self.binary_expression(body, nesting + 1)?;
                let close = self.take(TokenKind::CloseBracket)?;
                let span = UntrustedSpan {
                    file: self.file,
                    start: body.expressions[id as usize].span.start,
                    end: close.span().end(),
                };
                id = self.push_expression(
                    body,
                    syntax::RawExpressionSyntax {
                        span,
                        kind: syntax::RawExpressionKind::Index {
                            base: id,
                            open_bracket_span: raw(open),
                            index,
                            close_bracket_span: raw(close),
                        },
                    },
                )?;
                depth = depth.max(index_depth) + 1;
            } else {
                break;
            }
        }
        Ok((id, depth))
    }

    fn call(&mut self, body: &mut Body, nesting: u32) -> Result<(u32, u32), ParseError> {
        let callee = self.name(super::super::names::Role::Runtime)?;
        let type_arguments = self.type_arguments(0)?;
        let open = self.take(TokenKind::OpenParen)?;
        let count = super::super::super::collections::bounds(&self.tokens, self.position - 1)
            .map_or(0, |(_, count)| count);
        let required = match callee.text.as_str() {
            "clone" | "shared" | "downgrade" | "borrow" | "borrowMut" => Some(1),
            "push" => Some(2),
            _ => None,
        };
        if required.is_some() && type_arguments.is_some() {
            return Err(unsupported(Some(open), "intrinsic type arguments are excluded"));
        }
        if required.is_some_and(|required| count != required) {
            return Err(unsupported(Some(open), "wrong intrinsic argument count"));
        }
        if count > syntax::MAX_PARAMETERS_PER_FUNCTION {
            return Err(resource("call exceeds the argument limit"));
        }
        let mut arguments = Vec::new();
        let mut commas = Vec::new();
        let mut depth = 0;
        while self.current().is_some_and(|token| token.kind() != TokenKind::CloseParen) {
            if arguments.len() >= syntax::MAX_PARAMETERS_PER_FUNCTION {
                return Err(resource("call exceeds the argument limit"));
            }
            let (argument, argument_depth) = self.binary_expression(body, nesting + 1)?;
            arguments.push(argument);
            depth = depth.max(argument_depth);
            let Some(comma) = self.maybe(TokenKind::Comma) else {
                break;
            };
            commas.push(comma);
        }
        let close = self.take(TokenKind::CloseParen)?;
        let span =
            UntrustedSpan { file: self.file, start: callee.span.start, end: close.span().end() };
        let kind = match (callee.text.as_str(), arguments.as_slice()) {
            ("clone", &[value]) => syntax::RawExpressionKind::Clone {
                keyword_span: callee.span,
                open_paren_span: raw(open),
                value,
                close_paren_span: raw(close),
            },
            ("shared", &[value]) => syntax::RawExpressionKind::Shared {
                keyword_span: callee.span,
                open_paren_span: raw(open),
                value,
                close_paren_span: raw(close),
            },
            ("downgrade", &[value]) => syntax::RawExpressionKind::Downgrade {
                keyword_span: callee.span,
                open_paren_span: raw(open),
                value,
                close_paren_span: raw(close),
            },
            ("borrow", &[value]) => syntax::RawExpressionKind::Borrow {
                keyword_span: callee.span,
                open_paren_span: raw(open),
                value,
                close_paren_span: raw(close),
            },
            ("borrowMut", &[value]) => syntax::RawExpressionKind::BorrowMut {
                keyword_span: callee.span,
                open_paren_span: raw(open),
                value,
                close_paren_span: raw(close),
            },
            ("push", &[vector, value]) => syntax::RawExpressionKind::VecPush {
                keyword_span: callee.span,
                open_paren_span: raw(open),
                vector,
                comma_span: raw(commas[0]),
                value,
                close_paren_span: raw(close),
            },
            ("clone" | "shared" | "downgrade" | "borrow" | "borrowMut" | "push", _) => {
                return Err(unsupported(Some(open), "wrong intrinsic argument count"));
            }
            _ => syntax::RawExpressionKind::Call {
                callee,
                type_arguments,
                open_paren_span: raw(open),
                arguments,
                close_paren_span: raw(close),
            },
        };
        let id = self.push_expression(body, syntax::RawExpressionSyntax { span, kind })?;
        Ok((id, depth + 1))
    }

    pub(super) fn push_expression(
        &self,
        body: &mut Body,
        expression: syntax::RawExpressionSyntax,
    ) -> Result<u32, ParseError> {
        if body.expressions.len() >= syntax::MAX_EXPRESSIONS_PER_FUNCTION
            || self.previous_expressions + body.expressions.len()
                >= syntax::MAX_EXPRESSIONS_PER_PROJECT
        {
            return Err(resource(
                if body.expressions.len() >= syntax::MAX_EXPRESSIONS_PER_FUNCTION {
                    "function exceeds the expression limit"
                } else {
                    "project exceeds the expression limit"
                },
            ));
        }
        let id = u32::try_from(body.expressions.len()).expect("bounded expressions");
        body.expressions.push(expression);
        Ok(id)
    }
}

fn precedence(kind: TokenKind) -> Option<u8> {
    match kind {
        TokenKind::StrictEqual | TokenKind::StrictNotEqual => Some(1),
        TokenKind::LessThan
        | TokenKind::LessEqual
        | TokenKind::GreaterThan
        | TokenKind::GreaterEqual => Some(2),
        TokenKind::Plus | TokenKind::Minus => Some(3),
        TokenKind::Asterisk => Some(4),
        _ => None,
    }
}
