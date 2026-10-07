//! Source-spelled bounded lists; these contain no type resolution or trait authority.

use super::{FileParser, ParseError, raw, resource, syntax, unsupported};
use crate::native_lexer::{Keyword, TokenKind};
use zryna_source::UntrustedSpan;

impl FileParser<'_> {
    pub(super) fn type_parameters(
        &mut self,
    ) -> Result<Option<syntax::RawTypeParameterList>, ParseError> {
        let Some(less) = self.maybe(TokenKind::LessThan) else { return Ok(None) };
        let mut parameters = Vec::new();
        let mut comma_spans = Vec::new();
        loop {
            if parameters.len() >= syntax::MAX_TYPE_PARAMETERS {
                return Err(unsupported(
                    self.current(),
                    "at most two type parameters are admitted",
                ));
            }
            let name = self.name(super::names::Role::Parameter)?;
            let extends = self.take(TokenKind::Keyword(Keyword::Extends))?;
            let bound = self.name(super::names::Role::Type)?;
            if super::names::type_diversion(&bound.text) {
                return Err(unsupported(self.current(), "bound must be a named type"));
            }
            parameters.push(syntax::RawTypeParameter {
                span: UntrustedSpan {
                    file: self.file,
                    start: name.span.start,
                    end: bound.span.end,
                },
                name,
                extends_span: raw(extends),
                bound,
            });
            let Some(comma) = self.maybe(TokenKind::Comma) else { break };
            comma_spans.push(raw(comma));
            if self.current().is_some_and(|token| token.kind() == TokenKind::GreaterThan) {
                break;
            }
        }
        let greater = self.take(TokenKind::GreaterThan)?;
        Ok(Some(syntax::RawTypeParameterList {
            span: UntrustedSpan {
                file: self.file,
                start: less.span().start(),
                end: greater.span().end(),
            },
            less_than_span: raw(less),
            parameters,
            comma_spans,
            greater_than_span: raw(greater),
        }))
    }

    pub(super) fn type_arguments(
        &mut self,
        depth: u32,
    ) -> Result<Option<syntax::RawTypeArgumentList>, ParseError> {
        let Some(less) = self.maybe(TokenKind::LessThan) else { return Ok(None) };
        if depth > syntax::MAX_NESTING_DEPTH {
            return Err(resource("type syntax exceeds the nesting limit"));
        }
        let mut arguments = Vec::new();
        let mut comma_spans = Vec::new();
        loop {
            if arguments.len() >= syntax::MAX_TYPE_ARGUMENTS {
                return Err(unsupported(self.current(), "at most two type arguments are admitted"));
            }
            arguments.push(self.type_at_depth(depth + 1)?);
            let Some(comma) = self.maybe(TokenKind::Comma) else { break };
            comma_spans.push(raw(comma));
            if self.current().is_some_and(|token| {
                matches!(token.kind(), TokenKind::GreaterThan | TokenKind::GreaterEqual)
            }) {
                break;
            }
        }
        let greater = self.type_greater_than()?;
        Ok(Some(syntax::RawTypeArgumentList {
            span: UntrustedSpan { file: self.file, start: less.span().start(), end: greater.end },
            less_than_span: raw(less),
            arguments,
            comma_spans,
            greater_than_span: greater,
        }))
    }

    /// Look ahead without allocating types or consuming parser inventory.
    pub(super) fn call_open(&self, name: usize) -> Option<usize> {
        let next = self.tokens.get(name + 1)?;
        if next.kind() == TokenKind::OpenParen {
            return Some(name + 1);
        }
        if next.kind() != TokenKind::LessThan {
            return None;
        }
        let end = super::super::collections::generic_end(&self.tokens, name + 1)?;
        (self.tokens.get(end)?.kind() == TokenKind::OpenParen).then_some(end)
    }
}
