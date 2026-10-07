//! Source-ordered typed local, return, assignment and expression statements.

use super::super::{FileParser, ParseError, raw, resource, syntax, unsupported};
use super::Body;
use crate::native_lexer::{Keyword, TokenKind};
use zryna_source::UntrustedSpan;

impl FileParser<'_> {
    fn local_room(&self, locals: usize) -> Result<(), ParseError> {
        if locals >= 4_096 || self.previous_locals + locals >= 65_536 {
            return Err(resource(if locals >= 4_096 {
                "function exceeds the local limit"
            } else {
                "project exceeds the local limit"
            }));
        }
        Ok(())
    }

    pub(super) fn statement(
        &mut self,
        body: &mut Body,
        block_depth: u32,
    ) -> Result<syntax::RawStatementSyntax, ParseError> {
        let first = self.current().ok_or_else(|| unsupported(None, "missing statement"))?;
        let kind = match first.kind() {
            TokenKind::Keyword(Keyword::Return) => {
                self.position += 1;
                if self.current().is_some_and(|next| {
                    super::super::super::has_line_break(
                        self.text,
                        first.span().end(),
                        next.span().start(),
                    )
                }) {
                    return Err(unsupported(Some(first), "line terminator after return"));
                }
                let value = self.expression(body, block_depth)?;
                let semicolon = self.take(TokenKind::Semicolon)?;
                syntax::RawStatementKind::Return {
                    keyword_span: raw(first),
                    value,
                    semicolon_span: raw(semicolon),
                }
            }
            TokenKind::Keyword(Keyword::Const | Keyword::Let) => {
                self.local_room(body.locals)?;
                body.locals += 1;
                self.position += 1;
                let name = self.name(super::super::names::Role::Local)?;
                self.take(TokenKind::Colon)?;
                let type_syntax = self.type_syntax()?;
                let equals_span = self.initializer_equals()?;
                let initializer = self.expression(body, block_depth)?;
                let semicolon = self.take(TokenKind::Semicolon)?;
                syntax::RawStatementKind::LocalDeclaration {
                    keyword_span: raw(first),
                    mutable: first.kind() == TokenKind::Keyword(Keyword::Let),
                    name,
                    type_syntax,
                    equals_span,
                    initializer,
                    semicolon_span: raw(semicolon),
                }
            }
            _ => {
                let target = self.expression(body, block_depth)?;
                if let Some(equals) = self.maybe(TokenKind::Equals) {
                    if let syntax::RawExpressionKind::Reference { name } =
                        &body.expressions[target as usize].kind
                    {
                        self.name_role(name, super::super::names::Role::Assignment)?;
                    }
                    let value = self.expression(body, block_depth)?;
                    let semicolon = self.take(TokenKind::Semicolon)?;
                    syntax::RawStatementKind::Assignment {
                        target,
                        equals_span: raw(equals),
                        value,
                        semicolon_span: raw(semicolon),
                    }
                } else {
                    let semicolon = self.take(TokenKind::Semicolon)?;
                    syntax::RawStatementKind::ExpressionStatement {
                        expression: target,
                        semicolon_span: raw(semicolon),
                    }
                }
            }
        };
        let end = self.tokens[self.position - 1].span().end();
        Ok(syntax::RawStatementSyntax {
            span: UntrustedSpan { file: self.file, start: first.span().start(), end },
            kind,
        })
    }
}
