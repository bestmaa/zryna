//! Bounded synchronization after a rejected protocol-v2 declaration.

use zryna_syntax::v2 as syntax;

use super::{FileParser, ParseError};
use crate::native_lexer::{Keyword, TokenKind};

impl FileParser<'_> {
    pub(super) fn retain_error(&mut self, error: &ParseError) {
        self.retain_diagnostic(raw_diagnostic(error));
    }

    pub(super) fn retain_diagnostic(&mut self, diagnostic: syntax::RawProviderDiagnostic) {
        self.diagnostics.add(diagnostic);
    }
}

/// Retain the same earliest 255 source diagnostics as the bootstrap collector.
#[derive(Default)]
pub(super) struct Diagnostics {
    retained: Vec<syntax::RawProviderDiagnostic>,
    truncated: bool,
}

impl Diagnostics {
    pub(super) fn is_empty(&self) -> bool {
        self.retained.is_empty()
    }

    pub(super) fn add(&mut self, diagnostic: syntax::RawProviderDiagnostic) {
        if self.retained.len() < syntax::MAX_PROVIDER_DIAGNOSTICS - 1 {
            self.retained.push(diagnostic);
            return;
        }
        self.truncated = true;
        let (worst, _) = self
            .retained
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| compare_diagnostics(left, right))
            .expect("nonempty retained diagnostics");
        if compare_diagnostics(&diagnostic, &self.retained[worst]).is_lt() {
            self.retained[worst] = diagnostic;
        }
    }

    pub(super) fn append(&mut self, other: &mut Self) {
        self.truncated |= other.truncated;
        other.truncated = false;
        for diagnostic in other.retained.drain(..) {
            self.add(diagnostic);
        }
    }

    pub(super) fn finish(mut self) -> Vec<syntax::RawProviderDiagnostic> {
        self.retained.sort_by(compare_diagnostics);
        if self.truncated {
            self.retained.push(syntax::RawProviderDiagnostic {
                code: "ZRYNA-F2003".to_owned(),
                severity: zryna_diagnostics::Severity::Error,
                location: syntax::RawDiagnosticLocation::Global,
                message: "frontend diagnostics exceeded the deterministic limit".to_owned(),
                guidance: "reduce unsupported or malformed source before analysis".to_owned(),
            });
        }
        self.retained
    }
}

fn compare_diagnostics(
    left: &syntax::RawProviderDiagnostic,
    right: &syntax::RawProviderDiagnostic,
) -> std::cmp::Ordering {
    fn key(diagnostic: &syntax::RawProviderDiagnostic) -> (i64, i64, i64, &str, &str, &str) {
        let (file, start, end) = match &diagnostic.location {
            syntax::RawDiagnosticLocation::Source { span } => {
                (i64::from(span.file), i64::from(span.start), i64::from(span.end))
            }
            syntax::RawDiagnosticLocation::Global => (-1, -1, -1),
        };
        (file, start, end, &diagnostic.code, &diagnostic.message, &diagnostic.guidance)
    }
    key(left).cmp(&key(right))
}

/// Consume one rejected statement without crossing a containing or mismatched delimiter.
pub(super) fn skip_statement(
    parser: &mut FileParser<'_>,
    checkpoint: usize,
) -> Result<(), ParseError> {
    parser.position = checkpoint;
    let mut closers = Vec::new();
    while let Some(token) = parser.current() {
        if closers.is_empty() && token.kind() == TokenKind::CloseBrace {
            return Ok(());
        }
        match token.kind() {
            TokenKind::OpenBrace => closers.push(TokenKind::CloseBrace),
            TokenKind::OpenBracket => closers.push(TokenKind::CloseBracket),
            TokenKind::OpenParen => closers.push(TokenKind::CloseParen),
            TokenKind::CloseBrace | TokenKind::CloseBracket | TokenKind::CloseParen => {
                if closers.pop() != Some(token.kind()) {
                    return Err(parser.error_here("ZRYNA-F2002", "mismatched recovery delimiter"));
                }
            }
            TokenKind::Semicolon if closers.is_empty() => {
                parser.position += 1;
                return Ok(());
            }
            _ => {}
        }
        parser.position += 1;
        if closers.is_empty()
            && parser.current().is_some_and(|next| {
                next.kind() == TokenKind::Keyword(Keyword::Return)
                    && super::has_line_break(parser.text, token.span().end(), next.span().start())
            })
        {
            return Ok(());
        }
    }
    Ok(())
}

pub(super) fn primitive_kind(spelling: &str) -> Option<&'static str> {
    Some(match spelling {
        "unknown" => "UnknownKeyword",
        "never" => "NeverKeyword",
        "number" => "NumberKeyword",
        "string" => "StringKeyword",
        "boolean" => "BooleanKeyword",
        "symbol" => "SymbolKeyword",
        "bigint" => "BigIntKeyword",
        "undefined" => "UndefinedKeyword",
        "object" => "ObjectKeyword",
        _ => return None,
    })
}

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
