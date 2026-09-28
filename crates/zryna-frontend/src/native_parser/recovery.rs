//! Bounded synchronization after a rejected protocol-v2 declaration.

use zryna_syntax::v2 as syntax;

use super::{FileParser, ParseError};
use crate::native_lexer::{Keyword, TokenKind};

pub(super) struct SignatureDiagnostics {
    pub(super) diagnostics: Vec<syntax::RawProviderDiagnostic>,
    pub(super) parameter_count: usize,
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

/// Rescan only a structurally complete signature after its first primitive-type error.
/// The function remains rejected; this supplies the worker's additional diagnostic spans.
pub(super) fn primitive_annotations(
    parser: &FileParser<'_>,
    checkpoint: usize,
    function_index: usize,
) -> Option<Result<SignatureDiagnostics, ParseError>> {
    let mut position = checkpoint;
    take(parser, &mut position, TokenKind::Keyword(Keyword::Export))?;
    take(parser, &mut position, TokenKind::Keyword(Keyword::Function))?;
    take(parser, &mut position, TokenKind::Identifier)?;
    take(parser, &mut position, TokenKind::OpenParen)?;
    let mut diagnostics = Vec::new();
    let mut parameter_count = 0;
    if parser.tokens.get(position)?.kind() != TokenKind::CloseParen {
        loop {
            if parameter_count >= syntax::MAX_PARAMETERS_PER_FUNCTION {
                return Some(Err(super::resource(
                    "function parameter inventory exceeds protocol-v2 limit",
                )));
            }
            take(parser, &mut position, TokenKind::Identifier)?;
            if parser.tokens.get(position)?.kind() == TokenKind::Colon {
                position += 1;
                let annotation = take(parser, &mut position, TokenKind::Identifier)?;
                if let Some(kind) = primitive_kind(parser.spelling(annotation)) {
                    if diagnostics.len() >= syntax::MAX_PROVIDER_DIAGNOSTICS {
                        return Some(Err(super::resource(
                            "parser diagnostics exceed protocol-v2 limit",
                        )));
                    }
                    diagnostics.push(primitive_diagnostic(
                        annotation,
                        &format!("parameter {parameter_count} annotation"),
                        kind,
                    ));
                }
            }
            parameter_count += 1;
            if parser.tokens.get(position)?.kind() != TokenKind::Comma {
                break;
            }
            position += 1;
            if parser.tokens.get(position)?.kind() == TokenKind::CloseParen {
                break;
            }
        }
    }
    take(parser, &mut position, TokenKind::CloseParen)?;
    if parser.tokens.get(position)?.kind() == TokenKind::Colon {
        position += 1;
        let annotation = take(parser, &mut position, TokenKind::Identifier)?;
        if let Some(kind) = primitive_kind(parser.spelling(annotation)) {
            if diagnostics.len() >= syntax::MAX_PROVIDER_DIAGNOSTICS {
                return Some(Err(super::resource("parser diagnostics exceed protocol-v2 limit")));
            }
            diagnostics.push(primitive_diagnostic(
                annotation,
                &format!("function {function_index} result annotation"),
                kind,
            ));
        }
    }
    take(parser, &mut position, TokenKind::OpenBrace)?;
    Some(Ok(SignatureDiagnostics { diagnostics, parameter_count }))
}

fn take(
    parser: &FileParser<'_>,
    position: &mut usize,
    expected: TokenKind,
) -> Option<crate::native_lexer::Token> {
    let token = *parser.tokens.get(*position)?;
    if token.kind() != expected {
        return None;
    }
    *position += 1;
    Some(token)
}

fn primitive_diagnostic(
    token: crate::native_lexer::Token,
    context: &str,
    kind: &str,
) -> syntax::RawProviderDiagnostic {
    syntax::RawProviderDiagnostic {
        code: "ZRYNA-F2002".to_owned(),
        severity: zryna_diagnostics::Severity::Error,
        location: syntax::RawDiagnosticLocation::Source { span: super::raw(token) },
        message: format!("{context} uses unsupported syntax '{kind}'"),
        guidance: "use only the documented protocol-v2 bootstrap syntax".to_owned(),
    }
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
