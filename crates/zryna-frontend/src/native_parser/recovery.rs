//! Bounded synchronization after a rejected protocol-v2 declaration.

use zryna_syntax::v2 as syntax;

use super::{FileParser, ParseError};
use crate::native_lexer::{Keyword, TokenKind};

pub(super) fn newline_expression_statement(
    parser: &FileParser<'_>,
) -> Option<syntax::RawProviderDiagnostic> {
    let value = parser.current()?;
    let semicolon = *parser.tokens.get(parser.position + 1)?;
    if value.kind() != TokenKind::DecimalInteger || semicolon.kind() != TokenKind::Semicolon {
        return None;
    }
    Some(syntax::RawProviderDiagnostic {
        code: "ZRYNA-F2002".to_owned(),
        severity: zryna_diagnostics::Severity::Error,
        location: syntax::RawDiagnosticLocation::Source {
            span: zryna_source::UntrustedSpan {
                file: parser.file,
                start: value.span().start(),
                end: semicolon.span().end(),
            },
        },
        message: "statement uses unsupported syntax 'ExpressionStatement'".to_owned(),
        guidance: "use only the documented protocol-v2 bootstrap syntax".to_owned(),
    })
}

pub(super) fn skip_to_next_function(parser: &mut FileParser<'_>, checkpoint: usize) {
    parser.position = checkpoint;
    let mut closers = Vec::new();
    while let Some(token) = parser.current() {
        match token.kind() {
            TokenKind::OpenBrace => closers.push(TokenKind::CloseBrace),
            TokenKind::OpenBracket => closers.push(TokenKind::CloseBracket),
            TokenKind::OpenParen => closers.push(TokenKind::CloseParen),
            TokenKind::CloseBrace | TokenKind::CloseBracket | TokenKind::CloseParen
                if closers.pop() != Some(token.kind()) =>
            {
                parser.position = parser.tokens.len();
                return;
            }
            _ => {}
        }
        parser.position += 1;
        if closers.is_empty()
            && parser
                .current()
                .is_some_and(|next| next.kind() == TokenKind::Keyword(Keyword::Export))
        {
            break;
        }
    }
}

pub(super) fn raw_diagnostic(error: &ParseError) -> syntax::RawProviderDiagnostic {
    let diagnostic = error.diagnostic();
    let location =
        diagnostic.primary_span().map_or(syntax::RawDiagnosticLocation::Global, |span| {
            syntax::RawDiagnosticLocation::Source {
                span: zryna_source::UntrustedSpan {
                    file: span.file().index(),
                    start: span.start(),
                    end: span.end(),
                },
            }
        });
    syntax::RawProviderDiagnostic {
        code: diagnostic.code().to_owned(),
        severity: diagnostic.severity(),
        location,
        message: diagnostic.message().to_owned(),
        guidance: diagnostic.guidance().to_owned(),
    }
}
