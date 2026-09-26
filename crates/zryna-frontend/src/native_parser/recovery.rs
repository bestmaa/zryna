//! Bounded synchronization after a rejected protocol-v2 declaration.

use zryna_syntax::v2 as syntax;

use super::{FileParser, ParseError};
use crate::native_lexer::{Keyword, TokenKind};

pub(super) fn skip_to_next_function(parser: &mut FileParser<'_>, checkpoint: usize) {
    parser.position = checkpoint;
    let mut braces = 0_usize;
    while let Some(token) = parser.current() {
        match token.kind() {
            TokenKind::OpenBrace => braces = braces.saturating_add(1),
            TokenKind::CloseBrace => braces = braces.saturating_sub(1),
            _ => {}
        }
        parser.position += 1;
        if braces == 0
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
