//! Bounded postorder expression construction for the protocol-v2 slice.

use zryna_source::UntrustedSpan;
use zryna_syntax::v2 as syntax;

use super::{FileParser, ParseError, error_at, raw, resource};
use crate::native_lexer::{Keyword, TokenKind};

pub(super) fn addition(
    parser: &mut FileParser<'_>,
    expressions: &mut Vec<syntax::RawExpressionSyntax>,
) -> Result<u32, ParseError> {
    let mut lhs = atom(parser, expressions)?;
    let mut depth = 1_u32;
    while let Some(operator) = parser.maybe(TokenKind::Plus) {
        if depth >= syntax::MAX_EXPRESSION_DEPTH {
            return Err(error_at(
                operator,
                "ZRYNA-F2002",
                "expression depth exceeds protocol-v2 limit",
            ));
        }
        let rhs = atom(parser, expressions)?;
        let left = expressions[lhs as usize].span;
        let right = expressions[rhs as usize].span;
        lhs = push(
            expressions,
            syntax::RawExpressionSyntax {
                span: UntrustedSpan { file: parser.file, start: left.start, end: right.end },
                kind: syntax::RawExpressionKind::Addition {
                    operator_span: raw(operator),
                    lhs,
                    rhs,
                },
            },
        )?;
        depth += 1;
    }
    Ok(lhs)
}

fn atom(
    parser: &mut FileParser<'_>,
    expressions: &mut Vec<syntax::RawExpressionSyntax>,
) -> Result<u32, ParseError> {
    let token =
        parser.current().ok_or_else(|| parser.error_here("ZRYNA-F2002", "missing expression"))?;
    let expression = match token.kind() {
        TokenKind::Identifier => {
            let name = parser.identifier()?;
            reject_call(parser, token)?;
            syntax::RawExpressionSyntax {
                span: name.span,
                kind: syntax::RawExpressionKind::Reference { name },
            }
        }
        TokenKind::Keyword(Keyword::True | Keyword::False) => {
            parser.position += 1;
            syntax::RawExpressionSyntax {
                span: raw(token),
                kind: syntax::RawExpressionKind::BoolLiteral {
                    value: token.kind() == TokenKind::Keyword(Keyword::True),
                },
            }
        }
        TokenKind::DecimalInteger => {
            parser.position += 1;
            let spelling = parser.spelling(token);
            if spelling.len() > syntax::MAX_LITERAL_BYTES
                || (spelling.len() > 1 && spelling.starts_with('0'))
            {
                return Err(error_at(token, "ZRYNA-F2002", "integer spelling is not canonical"));
            }
            syntax::RawExpressionSyntax {
                span: raw(token),
                kind: syntax::RawExpressionKind::I32Literal { spelling: spelling.to_owned() },
            }
        }
        TokenKind::StringLiteral => {
            return Err(parser.error_between(
                token,
                token,
                "expression uses unsupported syntax 'StringLiteral'",
            ));
        }
        TokenKind::Minus => {
            parser.position += 1;
            let digits = parser.take(TokenKind::DecimalInteger)?;
            if token.span().end() != digits.span().start() {
                return Err(error_at(
                    token,
                    "ZRYNA-F2002",
                    "signed integer must have no intervening trivia",
                ));
            }
            let spelling =
                &parser.text[token.span().start() as usize..digits.span().end() as usize];
            if spelling.len() > syntax::MAX_LITERAL_BYTES || spelling.starts_with("-0") {
                return Err(error_at(token, "ZRYNA-F2002", "integer spelling is not canonical"));
            }
            syntax::RawExpressionSyntax {
                span: UntrustedSpan {
                    file: parser.file,
                    start: token.span().start(),
                    end: digits.span().end(),
                },
                kind: syntax::RawExpressionKind::I32Literal { spelling: spelling.to_owned() },
            }
        }
        TokenKind::OpenParen => {
            let mut depth = 0_usize;
            for next in parser.tokens[parser.position..].iter().copied() {
                match next.kind() {
                    TokenKind::OpenParen => depth += 1,
                    TokenKind::CloseParen => {
                        depth -= 1;
                        if depth == 0 {
                            return Err(parser.error_between(
                                token,
                                next,
                                "expression uses unsupported syntax 'ParenthesizedExpression'",
                            ));
                        }
                    }
                    TokenKind::OpenBrace
                    | TokenKind::CloseBrace
                    | TokenKind::OpenBracket
                    | TokenKind::CloseBracket
                    | TokenKind::Semicolon => break,
                    _ => {}
                }
            }
            return Err(parser.error_here("ZRYNA-F2002", "unsupported protocol-v2 expression"));
        }
        _ => return Err(parser.error_here("ZRYNA-F2002", "unsupported protocol-v2 expression")),
    };
    reject_multiplication(parser, token)?;
    push(expressions, expression)
}

fn reject_call(
    parser: &FileParser<'_>,
    first: crate::native_lexer::Token,
) -> Result<(), ParseError> {
    if parser.current().is_some_and(|next| next.kind() == TokenKind::OpenParen) {
        let close = call_end(parser).ok_or_else(|| {
            parser.error_here("ZRYNA-F2002", "unsupported protocol-v2 expression")
        })?;
        return Err(parser.error_between(
            first,
            close,
            "expression uses unsupported syntax 'CallExpression'",
        ));
    }
    Ok(())
}

fn reject_multiplication(
    parser: &FileParser<'_>,
    first: crate::native_lexer::Token,
) -> Result<(), ParseError> {
    if parser.current().is_some_and(|next| next.kind() == TokenKind::Asterisk) {
        let end = multiplication_end(parser).ok_or_else(|| {
            parser.error_here("ZRYNA-F2002", "unsupported protocol-v2 expression")
        })?;
        return Err(parser.error_between(
            first,
            end,
            "expression uses unsupported syntax 'BinaryExpression'",
        ));
    }
    Ok(())
}

fn call_end(parser: &FileParser<'_>) -> Option<crate::native_lexer::Token> {
    let mut closers = Vec::new();
    for token in parser.tokens[parser.position..].iter().copied() {
        match token.kind() {
            TokenKind::OpenParen => closers.push(TokenKind::CloseParen),
            TokenKind::OpenBracket => closers.push(TokenKind::CloseBracket),
            TokenKind::OpenBrace => closers.push(TokenKind::CloseBrace),
            TokenKind::CloseParen | TokenKind::CloseBracket | TokenKind::CloseBrace => {
                if closers.pop() != Some(token.kind()) {
                    return None;
                }
                if closers.is_empty() {
                    return Some(token);
                }
            }
            TokenKind::Semicolon if closers.is_empty() => return None,
            _ => {}
        }
    }
    None
}

fn multiplication_end(parser: &FileParser<'_>) -> Option<crate::native_lexer::Token> {
    let mut tokens = parser.tokens[parser.position..].iter().copied();
    let mut end = None;
    while tokens.next()?.kind() == TokenKind::Asterisk {
        let mut operand = tokens.next()?;
        if operand.kind() == TokenKind::Minus {
            operand = tokens.next()?;
            if operand.kind() != TokenKind::DecimalInteger {
                return None;
            }
        } else if !matches!(
            operand.kind(),
            TokenKind::Identifier
                | TokenKind::DecimalInteger
                | TokenKind::StringLiteral
                | TokenKind::Keyword(Keyword::True | Keyword::False)
        ) {
            return None;
        }
        end = Some(operand);
        if tokens.clone().next().is_none_or(|next| next.kind() != TokenKind::Asterisk) {
            break;
        }
    }
    end
}

fn push(
    expressions: &mut Vec<syntax::RawExpressionSyntax>,
    expression: syntax::RawExpressionSyntax,
) -> Result<u32, ParseError> {
    if expressions.len() >= syntax::MAX_EXPRESSIONS_PER_FUNCTION {
        return Err(resource("function expression inventory exceeds protocol-v2 limit"));
    }
    let index = u32::try_from(expressions.len())
        .map_err(|_| resource("expression index exceeds protocol-v2 range"))?;
    expressions.push(expression);
    Ok(index)
}
