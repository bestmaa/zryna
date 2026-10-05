//! Named imports retain the existing bounded syntax and exact UTF-8 spans.

use super::{FileParser, ParseError, raw, resource, syntax, unsupported, valid_specifier};
use crate::native_lexer::{Keyword, TokenKind};
use zryna_source::UntrustedSpan;

impl FileParser<'_> {
    pub(super) fn import(&mut self) -> Result<syntax::RawImportSyntax, ParseError> {
        let keyword = self.take(TokenKind::Keyword(Keyword::Import))?;
        self.take(TokenKind::OpenBrace)?;
        let count = super::collections::bounds(&self.tokens, self.position - 1)
            .map_or(0, |(_, count)| count);
        if count > syntax::MAX_IMPORTED_NAMES_PER_DECLARATION
            || self.previous_bindings + count > syntax::MAX_IMPORTED_NAMES_PER_PROJECT
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
                    return Err(unsupported(self.current(), "empty named import"));
                }
                break;
            }
            if bindings.len() >= syntax::MAX_IMPORTED_NAMES_PER_DECLARATION
                || self.previous_bindings + bindings.len() >= syntax::MAX_IMPORTED_NAMES_PER_PROJECT
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
            let local = if as_span.is_some() {
                self.name(super::names::Role::Data)?
            } else {
                imported.clone()
            };
            self.name_role(&local, super::names::Role::Data)?;
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
            return Err(unsupported(Some(literal), "unsupported module specifier"));
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
