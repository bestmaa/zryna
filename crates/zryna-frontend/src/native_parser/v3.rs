//! Internal protocol-v3 candidate construction from bound native tokens.
//!
//! Each entry consumes or rejects every nontrivia token in its admitted subset. Raw candidates
//! remain untrusted until the existing protocol-v3 verifier checks them; this is not a provider.

use std::fmt;

use zryna_diagnostics::Diagnostic;
use zryna_source::{SourceMap, Span, UntrustedSpan};
use zryna_syntax::v3 as syntax;

use crate::native_lexer::{Keyword, LexedProject, Token, TokenKind};

mod discovery;
mod straight_line;

pub use discovery::{RawModuleImports, discover_import_candidates};
pub use straight_line::parse_v3_straight_line_candidate;

/// One deterministic protocol-v3 candidate rejection.
#[derive(Clone, Debug)]
pub struct ParseError {
    diagnostic: Diagnostic,
}

impl ParseError {
    /// Returns the source-bound or resource diagnostic.
    #[must_use]
    pub const fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.diagnostic, formatter)
    }
}

impl std::error::Error for ParseError {}

/// Constructs an untrusted v3 candidate for functionless modules containing only named imports.
///
/// The complete bound token stream is consumed in source order. One unsupported or malformed
/// token rejects the whole project; no candidate with omitted imports or declarations is exposed.
/// The returned DTO must still pass [`syntax::verify_snapshot`] before any downstream use.
///
/// # Errors
///
/// Rejects foreign source authority, lexical diagnostics, unsupported syntax, and first-extra
/// import or binding inventory without returning a partial candidate.
pub fn parse_v3_import_candidate(
    sources: &SourceMap,
    lexed: &LexedProject,
) -> Result<syntax::RawProjectSyntaxSnapshot, ParseError> {
    if !lexed.is_bound_to(sources) || lexed.files().len() != sources.len() {
        return Err(failure("ZRYNA-F2002", "native tokens do not belong to this source map"));
    }
    if let Some(diagnostic) = lexed.diagnostics().first() {
        return Err(ParseError { diagnostic: diagnostic.clone() });
    }
    let mut files = Vec::with_capacity(lexed.files().len());
    let mut total_imports = 0_usize;
    let mut total_bindings = 0_usize;
    for file in lexed.files() {
        let source = sources
            .source(file.id())
            .ok_or_else(|| failure("ZRYNA-F2002", "native source file is unavailable"))?;
        if source.path() != file.path() {
            return Err(failure("ZRYNA-F2002", "native source path differs from the source map"));
        }
        let end = u32::try_from(source.text().len())
            .map_err(|_| failure("ZRYNA-F1002", "source length exceeds protocol-v3 limit"))?;
        let eof = sources
            .verify_span(UntrustedSpan { file: file.id().index(), start: end, end })
            .map_err(|_| failure("ZRYNA-F2002", "native source EOF is unavailable"))?;
        let mut parser = FileParser {
            sources,
            text: source.text(),
            tokens: file.tokens().collect(),
            position: 0,
            file: file.id().index(),
            eof,
        };
        let mut imports = Vec::new();
        while let Some(token) = parser.current() {
            if token.kind() != TokenKind::Keyword(Keyword::Import) {
                return Err(
                    parser.error_here("unsupported token in import-only protocol-v3 source")
                );
            }
            if imports.len() >= syntax::MAX_IMPORTS_PER_MODULE
                || total_imports >= syntax::MAX_IMPORTS_PER_PROJECT
            {
                return Err(resource(if imports.len() >= syntax::MAX_IMPORTS_PER_MODULE {
                    "module exceeds the import-declaration limit"
                } else {
                    "project exceeds the import-declaration limit"
                }));
            }
            let import = parser.import(total_bindings)?;
            total_imports += 1;
            total_bindings += import.bindings.len();
            imports.push(import);
        }
        files.push(syntax::RawSourceUnit {
            id: file.id().index(),
            path: file.path().as_str().to_owned(),
            imports,
            functions: Vec::new(),
        });
    }
    Ok(syntax::RawProjectSyntaxSnapshot {
        schema_version: syntax::PROTOCOL_VERSION,
        files,
        diagnostics: Vec::new(),
    })
}

struct FileParser<'a> {
    sources: &'a SourceMap,
    text: &'a str,
    tokens: Vec<Token>,
    position: usize,
    file: u32,
    eof: Span,
}

impl FileParser<'_> {
    fn current(&self) -> Option<Token> {
        self.tokens.get(self.position).copied()
    }

    fn error_here(&self, message: &'static str) -> ParseError {
        self.current().or_else(|| self.tokens.last().copied()).map_or_else(
            || failure("ZRYNA-F2002", message),
            |token| ParseError {
                diagnostic: Diagnostic::error_at(
                    "ZRYNA-F2002",
                    token.span(),
                    message,
                    "use only named imports in this internal protocol-v3 candidate",
                ),
            },
        )
    }

    fn take(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        let token = self.current().ok_or_else(|| self.error_here("incomplete named import"))?;
        if token.kind() != kind {
            return Err(self.error_here("unsupported named import syntax"));
        }
        self.position += 1;
        Ok(token)
    }

    fn maybe(&mut self, kind: TokenKind) -> Option<Token> {
        if self.current().is_some_and(|token| token.kind() == kind) {
            let token = self.current();
            self.position += 1;
            token
        } else {
            None
        }
    }

    fn spelling(&self, token: Token) -> &str {
        &self.text[token.span().start() as usize..token.span().end() as usize]
    }

    fn identifier(&mut self) -> Result<syntax::RawIdentifierSyntax, ParseError> {
        let token = self.take(TokenKind::Identifier)?;
        Ok(syntax::RawIdentifierSyntax { text: self.spelling(token).to_owned(), span: raw(token) })
    }

    fn import(&mut self, previous_bindings: usize) -> Result<syntax::RawImportSyntax, ParseError> {
        let keyword = self.take(TokenKind::Keyword(Keyword::Import))?;
        self.take(TokenKind::OpenBrace)?;
        let count = super::collections::bounds(&self.tokens, self.position - 1)
            .map_or(0, |(_, count)| count);
        if count > syntax::MAX_IMPORTED_NAMES_PER_DECLARATION
            || previous_bindings + count > syntax::MAX_IMPORTED_NAMES_PER_PROJECT
        {
            return Err(resource(if count > syntax::MAX_IMPORTED_NAMES_PER_DECLARATION {
                "import exceeds the imported-name limit"
            } else {
                "project exceeds the imported-name limit"
            }));
        }
        let mut bindings = Vec::new();
        loop {
            if self.current().is_some_and(|token| token.kind() == TokenKind::CloseBrace) {
                if bindings.is_empty() {
                    return Err(self.error_here("named import must contain a binding"));
                }
                break;
            }
            if bindings.len() >= syntax::MAX_IMPORTED_NAMES_PER_DECLARATION
                || previous_bindings + bindings.len() >= syntax::MAX_IMPORTED_NAMES_PER_PROJECT
            {
                return Err(resource(
                    if bindings.len() >= syntax::MAX_IMPORTED_NAMES_PER_DECLARATION {
                        "import exceeds the imported-name limit"
                    } else {
                        "project exceeds the imported-name limit"
                    },
                ));
            }
            let imported = self.identifier()?;
            let as_span = self.maybe(TokenKind::Keyword(Keyword::As)).map(raw);
            let local = if as_span.is_some() { self.identifier()? } else { imported.clone() };
            bindings.push(syntax::RawImportBindingSyntax {
                span: UntrustedSpan {
                    file: self.file,
                    start: imported.span.start,
                    end: local.span.end,
                },
                imported,
                local,
                as_span,
            });
            if self.maybe(TokenKind::Comma).is_none() {
                break;
            }
        }
        self.take(TokenKind::CloseBrace)?;
        let from = self.take(TokenKind::Keyword(Keyword::From))?;
        let literal = self.take(TokenKind::StringLiteral)?;
        let spelling = self.spelling(literal);
        let value = &spelling[1..spelling.len() - 1];
        if !valid_specifier(value) {
            return Err(ParseError {
                diagnostic: Diagnostic::error_at(
                    "ZRYNA-F2002",
                    literal.span(),
                    "unsupported module specifier",
                    "use an explicit relative unescaped .zry module specifier",
                ),
            });
        }
        let specifier = syntax::RawModuleSpecifierSyntax {
            text: value.to_owned(),
            token_span: raw(literal),
            value_span: UntrustedSpan {
                file: self.file,
                start: literal.span().start() + 1,
                end: literal.span().end() - 1,
            },
        };
        let semicolon = self.take(TokenKind::Semicolon)?;
        Ok(syntax::RawImportSyntax {
            span: UntrustedSpan {
                file: self.file,
                start: keyword.span().start(),
                end: semicolon.span().end(),
            },
            import_span: raw(keyword),
            bindings,
            from_span: raw(from),
            specifier,
            semicolon_span: raw(semicolon),
        })
    }
}

#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn valid_specifier(value: &str) -> bool {
    if value.is_empty()
        || value.len() > syntax::MAX_MODULE_SPECIFIER_BYTES
        || !value.is_ascii()
        || !(value.starts_with("./") || value.starts_with("../"))
        || !value.ends_with(".zry")
        || value.contains(['\\', '?', '#', '\0'])
        || value.contains("://")
    {
        return false;
    }
    let body = value.strip_prefix("./").unwrap_or_else(|| value.trim_start_matches("../"));
    !body.is_empty() && !value.split('/').any(str::is_empty)
}

fn raw(token: Token) -> UntrustedSpan {
    let span = token.span();
    UntrustedSpan { file: span.file().index(), start: span.start(), end: span.end() }
}

fn failure(code: &'static str, message: &'static str) -> ParseError {
    ParseError {
        diagnostic: Diagnostic::error(
            code,
            None,
            message,
            "use the supported native import-only syntax subset",
        ),
    }
}

fn resource(message: &'static str) -> ParseError {
    failure("ZRYNA-F1002", message)
}
