//! Data, ownership, and collection constructions for protocol v4.

use std::collections::BTreeSet;

use zryna_source::UntrustedSpan;
use zryna_syntax::v4 as syntax;

use crate::native_lexer::TokenKind;

use super::super::{FileParser, ParseError, raw, resource, unsupported, valid_name};
use super::Body;

impl FileParser<'_> {
    pub(super) fn enum_construction(
        &mut self,
        body: &mut Body,
        nesting: u32,
    ) -> Result<(u32, u32), ParseError> {
        let type_name = self.identifier()?;
        let dot = self.take(TokenKind::Dot)?;
        let variant = self.identifier()?;
        let open = self.take(TokenKind::OpenParen)?;
        if super::super::super::collections::bounds(&self.tokens, self.position - 1)
            .is_some_and(|(_, count)| count > 1)
        {
            return Err(unsupported(Some(open), "unsupported enum construction"));
        }
        let (payload, depth) =
            if self.current().is_some_and(|token| token.kind() != TokenKind::CloseParen) {
                let (id, depth) = self.binary_expression(body, nesting + 1)?;
                (Some(id), depth)
            } else {
                (None, 0)
            };
        let close = self.take(TokenKind::CloseParen)?;
        self.count_aggregate_operands(usize::from(payload.is_some()))?;
        let span =
            UntrustedSpan { file: self.file, start: type_name.span.start, end: close.span().end() };
        let id = self.push_expression(
            body,
            syntax::RawExpressionSyntax {
                span,
                kind: syntax::RawExpressionKind::EnumConstruction {
                    type_name,
                    dot_span: raw(dot),
                    variant,
                    open_paren_span: raw(open),
                    payload,
                    close_paren_span: raw(close),
                },
            },
        )?;
        Ok((id, depth + 1))
    }

    pub(super) fn struct_construction(
        &mut self,
        body: &mut Body,
        nesting: u32,
    ) -> Result<(u32, u32), ParseError> {
        let type_name = self.identifier()?;
        let open_paren = self.take(TokenKind::OpenParen)?;
        if let Some((_, count)) =
            super::super::super::collections::bounds(&self.tokens, self.position - 1)
            && count != 1
        {
            return Err(if count > syntax::MAX_PARAMETERS_PER_FUNCTION {
                resource("call exceeds the argument limit")
            } else {
                unsupported(self.current(), "unsupported aggregate construction")
            });
        }
        let open_brace = self.take(TokenKind::OpenBrace)?;
        let count = self.collection_count(
            syntax::MAX_INITIALIZERS_PER_CONSTRUCTION,
            "struct construction exceeds the initializer limit",
        )?;
        self.count_aggregate_operands(count)?;
        let mut fields = Vec::new();
        let mut seen = BTreeSet::new();
        let mut depth = 0;
        while self.current().is_some_and(|token| token.kind() != TokenKind::CloseBrace) {
            if fields.len() >= syntax::MAX_INITIALIZERS_PER_CONSTRUCTION {
                return Err(resource("struct construction exceeds the initializer limit"));
            }
            let name = self.identifier()?;
            if !seen.insert(name.text.clone()) {
                return Err(unsupported(self.current(), "duplicate struct initializer"));
            }
            let kind = if let Some(colon) = self.maybe(TokenKind::Colon) {
                let (value, value_depth) = self.binary_expression(body, nesting + 1)?;
                depth = depth.max(value_depth);
                syntax::RawFieldInitializerKind::Explicit { name, colon_span: raw(colon), value }
            } else {
                let value = self.push_expression(
                    body,
                    syntax::RawExpressionSyntax {
                        span: name.span,
                        kind: syntax::RawExpressionKind::Reference { name: name.clone() },
                    },
                )?;
                depth = depth.max(1);
                syntax::RawFieldInitializerKind::Shorthand { name, value }
            };
            let end = match &kind {
                syntax::RawFieldInitializerKind::Explicit { value, .. } => {
                    body.expressions[*value as usize].span.end
                }
                syntax::RawFieldInitializerKind::Shorthand { name, .. } => name.span.end,
            };
            let start = match &kind {
                syntax::RawFieldInitializerKind::Explicit { name, .. }
                | syntax::RawFieldInitializerKind::Shorthand { name, .. } => name.span.start,
            };
            fields.push(syntax::RawFieldInitializer {
                span: UntrustedSpan { file: self.file, start, end },
                kind,
            });
            if self.maybe(TokenKind::Comma).is_none() {
                break;
            }
        }
        let close_brace = self.take(TokenKind::CloseBrace)?;
        let close_paren = self.take(TokenKind::CloseParen)?;
        let span = UntrustedSpan {
            file: self.file,
            start: type_name.span.start,
            end: close_paren.span().end(),
        };
        let id = self.push_expression(
            body,
            syntax::RawExpressionSyntax {
                span,
                kind: syntax::RawExpressionKind::StructConstruction {
                    type_name,
                    open_paren_span: raw(open_paren),
                    open_brace_span: raw(open_brace),
                    fields,
                    close_brace_span: raw(close_brace),
                    close_paren_span: raw(close_paren),
                },
            },
        )?;
        Ok((id, depth + 1))
    }

    pub(super) fn typed_array_end(&self) -> Option<usize> {
        let first = self.current()?;
        if first.kind() != TokenKind::Identifier
            || !matches!(self.spelling(first), "Vec" | "FixedArray")
            || self.next()?.kind() != TokenKind::LessThan
        {
            return None;
        }
        let mut depth = 0_usize;
        for (index, token) in self.tokens.iter().enumerate().skip(self.position + 1) {
            match token.kind() {
                TokenKind::LessThan => depth += 1,
                TokenKind::GreaterThan => {
                    depth -= 1;
                    if depth == 0 {
                        return (self.tokens.get(index + 1)?.kind() == TokenKind::OpenParen
                            && self.tokens.get(index + 2)?.kind() == TokenKind::OpenBracket)
                            .then_some(index + 1);
                    }
                }
                _ => {}
            }
            if depth > syntax::MAX_NESTING_DEPTH as usize {
                return None;
            }
        }
        None
    }

    pub(super) fn typed_array(
        &mut self,
        body: &mut Body,
        nesting: u32,
        type_end: usize,
    ) -> Result<(u32, u32), ParseError> {
        let type_start = self.position;
        let first = self.current().expect("typed construction name");
        let is_vec = self.spelling(first) == "Vec";
        if super::super::super::collections::bounds(&self.tokens, type_end)
            .is_some_and(|(_, count)| count != 1)
        {
            return Err(unsupported(Some(first), "unsupported typed construction"));
        }
        let types = super::super::super::collections::generic_arguments(
            &self.tokens,
            type_start + 1,
            type_end,
        );
        if types != if is_vec { 1 } else { 2 } {
            return Err(unsupported(Some(first), "unsupported typed construction"));
        }
        self.position = type_end;
        let open_paren = self.take(TokenKind::OpenParen)?;
        let open_bracket = self.take(TokenKind::OpenBracket)?;
        self.collection_count(
            syntax::MAX_ELEMENTS_PER_CONSTRUCTION,
            "array construction exceeds the element limit",
        )?;
        let mut elements = Vec::new();
        let mut depth = 0;
        while self.current().is_some_and(|token| token.kind() != TokenKind::CloseBracket) {
            if elements.len() >= syntax::MAX_ELEMENTS_PER_CONSTRUCTION {
                return Err(resource("array construction exceeds the element limit"));
            }
            let (element, element_depth) = self.binary_expression(body, nesting + 1)?;
            elements.push(element);
            depth = depth.max(element_depth);
            if self.maybe(TokenKind::Comma).is_none() {
                break;
            }
        }
        let close_bracket = self.take(TokenKind::CloseBracket)?;
        let close_paren = self.take(TokenKind::CloseParen)?;
        self.count_aggregate_operands(elements.len())?;
        let expression_end = self.position;
        self.position = type_start;
        let type_syntax = self.type_syntax()?;
        if self.position != type_end {
            return Err(unsupported(Some(first), "unsupported typed construction"));
        }
        self.position = expression_end;
        let kind = if is_vec {
            syntax::RawExpressionKind::VecConstruction {
                type_syntax,
                open_paren_span: raw(open_paren),
                open_bracket_span: raw(open_bracket),
                elements,
                close_bracket_span: raw(close_bracket),
                close_paren_span: raw(close_paren),
            }
        } else {
            syntax::RawExpressionKind::FixedArrayConstruction {
                type_syntax,
                open_paren_span: raw(open_paren),
                open_bracket_span: raw(open_bracket),
                elements,
                close_bracket_span: raw(close_bracket),
                close_paren_span: raw(close_paren),
            }
        };
        let span = UntrustedSpan {
            file: self.file,
            start: first.span().start(),
            end: close_paren.span().end(),
        };
        let id = self.push_expression(body, syntax::RawExpressionSyntax { span, kind })?;
        Ok((id, depth + 1))
    }

    fn count_aggregate_operands(&mut self, count: usize) -> Result<(), ParseError> {
        self.aggregate_operands += count;
        if self.aggregate_operands > syntax::MAX_AGGREGATE_OPERANDS_PER_PROJECT {
            return Err(resource("project exceeds the aggregate-construction operand limit"));
        }
        Ok(())
    }

    pub(super) fn collection_count(
        &self,
        maximum: usize,
        message: &'static str,
    ) -> Result<usize, ParseError> {
        let (_, count) = super::super::super::collections::bounds(&self.tokens, self.position - 1)
            .ok_or_else(|| unsupported(self.current(), "unclosed collection"))?;
        if count > maximum {
            return Err(resource(message));
        }
        Ok(count)
    }

    pub(super) fn match_expression(
        &mut self,
        body: &mut Body,
        nesting: u32,
    ) -> Result<(u32, u32), ParseError> {
        let keyword = self.take(TokenKind::Identifier)?;
        let open_paren = self.take(TokenKind::OpenParen)?;
        if super::super::super::collections::bounds(&self.tokens, self.position - 1)
            .is_some_and(|(_, count)| count != 2)
        {
            return Err(unsupported(Some(keyword), "unsupported match expression"));
        }
        let (scrutinee, mut depth) = self.binary_expression(body, nesting + 1)?;
        self.take(TokenKind::Comma)?;
        let open_brace = self.take(TokenKind::OpenBrace)?;
        let count = self.collection_count(
            syntax::MAX_MATCH_ARMS_PER_EXPRESSION,
            "match exceeds the arm limit",
        )?;
        self.match_arms += count;
        if self.match_arms > syntax::MAX_MATCH_ARMS_PER_PROJECT {
            return Err(resource("project exceeds the match-arm limit"));
        }
        let mut arms = Vec::new();
        let mut seen = BTreeSet::new();
        while self.current().is_some_and(|token| token.kind() != TokenKind::CloseBrace) {
            let (arm, value_depth) = self.match_arm(body, nesting, &mut seen)?;
            depth = depth.max(value_depth);
            arms.push(arm);
            if self.maybe(TokenKind::Comma).is_none() {
                break;
            }
        }
        let close_brace = self.take(TokenKind::CloseBrace)?;
        let close_paren = self.take(TokenKind::CloseParen)?;
        let span = UntrustedSpan {
            file: self.file,
            start: keyword.span().start(),
            end: close_paren.span().end(),
        };
        let id = self.push_expression(
            body,
            syntax::RawExpressionSyntax {
                span,
                kind: syntax::RawExpressionKind::Match {
                    keyword_span: raw(keyword),
                    open_paren_span: raw(open_paren),
                    scrutinee,
                    close_paren_span: raw(close_paren),
                    open_brace_span: raw(open_brace),
                    arms,
                    close_brace_span: raw(close_brace),
                },
            },
        )?;
        Ok((id, depth + 1))
    }

    fn match_arm(
        &mut self,
        body: &mut Body,
        nesting: u32,
        seen: &mut BTreeSet<String>,
    ) -> Result<(syntax::RawMatchArm, u32), ParseError> {
        let key = self.take(TokenKind::StringLiteral)?;
        let spelling = self.spelling(key).to_owned();
        if !spelling.starts_with('"') || !spelling.ends_with('"') {
            return Err(unsupported(Some(key), "unsupported match arm key"));
        }
        let qualified = spelling[1..spelling.len() - 1].to_owned();
        let Some((type_text, variant_text)) = qualified.split_once('.') else {
            return Err(unsupported(Some(key), "unsupported match arm key"));
        };
        if !valid_name(type_text)
            || !valid_name(variant_text)
            || variant_text.contains('.')
            || !seen.insert(qualified.clone())
        {
            return Err(unsupported(Some(key), "unsupported match arm key"));
        }
        self.take(TokenKind::Colon)?;
        self.take(TokenKind::OpenParen)?;
        let binding = if self.current().is_some_and(|token| token.kind() == TokenKind::Identifier) {
            Some(self.identifier()?)
        } else {
            None
        };
        self.take(TokenKind::CloseParen)?;
        let arrow = self.take(TokenKind::FatArrow)?;
        let (value, depth) = self.binary_expression(body, nesting + 1)?;
        let start = key.span().start() + 1;
        let type_end = start + u32::try_from(type_text.len()).expect("bounded identifier");
        let variant_end = start + u32::try_from(qualified.len()).expect("bounded key");
        Ok((
            syntax::RawMatchArm {
                span: UntrustedSpan {
                    file: self.file,
                    start: key.span().start(),
                    end: body.expressions[value as usize].span.end,
                },
                type_name: syntax::RawIdentifierSyntax {
                    text: type_text.to_owned(),
                    span: UntrustedSpan { file: self.file, start, end: type_end },
                },
                dot_span: UntrustedSpan { file: self.file, start: type_end, end: type_end + 1 },
                variant: syntax::RawIdentifierSyntax {
                    text: variant_text.to_owned(),
                    span: UntrustedSpan { file: self.file, start: type_end + 1, end: variant_end },
                },
                binding,
                arrow_span: raw(arrow),
                value,
            },
            depth,
        ))
    }
}
