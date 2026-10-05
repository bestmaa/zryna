//! Source-ordered struct and enum syntax for protocol v5.

use std::collections::BTreeSet;

use super::syntax;
use zryna_source::UntrustedSpan;

use crate::native_lexer::{Keyword, TokenKind};

use super::{FileParser, ParseError, raw, resource, unsupported};

impl FileParser<'_> {
    fn member_inventory(&self) -> Result<(), ParseError> {
        let count = super::super::collections::separated_bounds(
            &self.tokens,
            self.position - 1,
            TokenKind::Semicolon,
            true,
        )
        .map_or(0, |(_, count)| count);
        if count > syntax::MAX_MEMBERS_PER_DECLARATION
            || self.previous_members + count > syntax::MAX_MEMBERS_PER_PROJECT
        {
            return Err(resource(if count > syntax::MAX_MEMBERS_PER_DECLARATION {
                "data declaration exceeds the member limit"
            } else {
                "project exceeds the data-member limit"
            }));
        }
        Ok(())
    }

    pub(super) fn data_declaration(
        &mut self,
        previous_declarations: usize,
    ) -> Result<syntax::RawDataDeclaration, ParseError> {
        let export = self.maybe(TokenKind::Keyword(Keyword::Export));
        let interface = self.take(TokenKind::Keyword(Keyword::Interface))?;
        let name = self.name(super::names::Role::Data)?;
        let type_parameters = self.type_parameters()?;
        let extends = self.take(TokenKind::Keyword(Keyword::Extends))?;
        let marker = self.identifier()?;
        let is_struct = match marker.text.as_str() {
            "ZrynaStruct" => true,
            "ZrynaEnum" => false,
            _ => return Err(unsupported(Some(interface), "unsupported data-declaration marker")),
        };
        let open = self.take(TokenKind::OpenBrace)?;
        self.member_inventory()?;
        let mut fields = Vec::new();
        let mut variants = Vec::new();
        let mut seen = BTreeSet::new();
        while self.current().is_some_and(|token| token.kind() != TokenKind::CloseBrace) {
            let member = self.identifier()?;
            if !seen.insert(member.text.clone()) {
                return Err(unsupported(self.current(), "duplicate data member"));
            }
            let colon = self.take(TokenKind::Colon)?;
            let (payload_type, none_span) = if !is_struct
                && self.current().is_some_and(|token| {
                    token.kind() == TokenKind::Identifier && self.spelling(token) == "ZrynaNone"
                })
                && self.next().is_some_and(|token| token.kind() == TokenKind::Semicolon)
            {
                let none = self.take(TokenKind::Identifier)?;
                (None, Some(raw(none)))
            } else {
                (Some(self.type_syntax()?), None)
            };
            let semicolon = self.take(TokenKind::Semicolon)?;
            let span = UntrustedSpan {
                file: self.file,
                start: member.span.start,
                end: semicolon.span().end(),
            };
            if is_struct {
                fields.push(syntax::RawDataField {
                    span,
                    name: member,
                    colon_span: raw(colon),
                    type_syntax: payload_type.expect("struct field has a type"),
                    semicolon_span: raw(semicolon),
                });
            } else {
                variants.push(syntax::RawEnumVariant {
                    span,
                    name: member,
                    colon_span: raw(colon),
                    payload_type,
                    none_span,
                    semicolon_span: raw(semicolon),
                });
            }
        }
        if fields.is_empty() && variants.is_empty() {
            return Err(unsupported(self.current(), "empty data declaration"));
        }
        let close = self.take(TokenKind::CloseBrace)?;
        if previous_declarations >= syntax::MAX_DATA_DECLARATIONS_PER_PROJECT {
            return Err(resource("project exceeds the nominal-declaration limit"));
        }
        let kind = if is_struct {
            syntax::RawDataDeclarationKind::Struct {
                interface_span: raw(interface),
                name,
                extends_span: raw(extends),
                marker_span: marker.span,
                open_brace_span: raw(open),
                fields,
                close_brace_span: raw(close),
            }
        } else {
            syntax::RawDataDeclarationKind::Enum {
                interface_span: raw(interface),
                name,
                extends_span: raw(extends),
                marker_span: marker.span,
                open_brace_span: raw(open),
                variants,
                close_brace_span: raw(close),
            }
        };
        Ok(syntax::RawDataDeclaration {
            span: UntrustedSpan {
                file: self.file,
                start: export.unwrap_or(interface).span().start(),
                end: close.span().end(),
            },
            export_span: export.map(raw),
            type_parameters,
            kind,
        })
    }
}
