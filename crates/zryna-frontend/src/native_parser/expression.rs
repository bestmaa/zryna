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
    while let Some(operator) = parser.maybe(TokenKind::Plus) {
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
        _ => return Err(parser.error_here("ZRYNA-F2002", "unsupported protocol-v2 expression")),
    };
    push(expressions, expression)
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
