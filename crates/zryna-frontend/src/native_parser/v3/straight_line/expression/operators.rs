//! Precedence and postorder construction for protocol-v3 scalar binary expressions.

use zryna_source::UntrustedSpan;
use zryna_syntax::v3 as syntax;

use crate::native_lexer::{Keyword, Token, TokenKind};

use super::super::{FileParser, ParseError, function_error_at, raw};
use super::push_expression;

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

fn kind(token: Token, lhs: u32, rhs: u32) -> syntax::RawExpressionKind {
    let operator_span = raw(token);
    match token.kind() {
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
        TokenKind::LessEqual => syntax::RawExpressionKind::LessEqual { operator_span, lhs, rhs },
        TokenKind::GreaterThan => {
            syntax::RawExpressionKind::GreaterThan { operator_span, lhs, rhs }
        }
        TokenKind::GreaterEqual => {
            syntax::RawExpressionKind::GreaterEqual { operator_span, lhs, rhs }
        }
        _ => unreachable!("only binary tokens enter the operator stack"),
    }
}

fn reduce(
    expressions: &mut Vec<syntax::RawExpressionSyntax>,
    previous_expressions: usize,
    values: &mut Vec<(u32, u32)>,
    operator: Token,
    file: u32,
) -> Result<(), ParseError> {
    let (rhs, rhs_depth) = values.pop().expect("binary operator has a right operand");
    let (lhs, lhs_depth) = values.pop().expect("binary operator has a left operand");
    let depth = lhs_depth.max(rhs_depth) + 1;
    let span = UntrustedSpan {
        file,
        start: expressions[lhs as usize].span.start,
        end: expressions[rhs as usize].span.end,
    };
    let index = push_expression(
        expressions,
        previous_expressions,
        syntax::RawExpressionSyntax { span, kind: kind(operator, lhs, rhs) },
    )?;
    values.push((index, depth));
    Ok(())
}

pub(super) fn parse(
    parser: &mut FileParser<'_>,
    expressions: &mut Vec<syntax::RawExpressionSyntax>,
    previous_expressions: usize,
    call_nesting: u32,
) -> Result<(u32, u32), ParseError> {
    let mut values = vec![parser.operand(expressions, previous_expressions, call_nesting)?];
    let mut operators: Vec<(Token, u8)> = Vec::new();
    while let Some(token) = parser.current() {
        let Some(next_precedence) = precedence(token.kind()) else {
            break;
        };
        parser.position += 1;
        if matches!(token.kind(), TokenKind::Minus | TokenKind::Plus)
            && parser.current().is_some_and(|next| {
                next.kind() == token.kind() && token.span().end() == next.span().start()
            })
        {
            let second = parser.current().expect("adjacent operator token");
            let rejected = parser
                .tokens
                .get(parser.position + 1)
                .copied()
                .filter(|next| {
                    matches!(
                        next.kind(),
                        TokenKind::Identifier
                            | TokenKind::DecimalInteger
                            | TokenKind::Keyword(Keyword::True | Keyword::False)
                    )
                })
                .unwrap_or(second);
            return Err(function_error_at(rejected, "unsupported increment or decrement"));
        }
        while operators.last().is_some_and(|(_, current)| *current >= next_precedence) {
            let (operator, _) = operators.pop().expect("pending operator");
            reduce(expressions, previous_expressions, &mut values, operator, parser.file)?;
        }
        operators.push((token, next_precedence));
        values.push(parser.operand(expressions, previous_expressions, call_nesting)?);
    }
    while let Some((operator, _)) = operators.pop() {
        reduce(expressions, previous_expressions, &mut values, operator, parser.file)?;
    }
    Ok(values.pop().expect("an expression has one root"))
}
