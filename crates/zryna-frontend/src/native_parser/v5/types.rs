//! Bounded postorder type occurrences, including nominal generic applications.

use super::{FileParser, ParseError, raw, resource, syntax, unsupported};
use crate::native_lexer::TokenKind;
use zryna_source::UntrustedSpan;

impl FileParser<'_> {
    pub(super) fn type_greater_than(&mut self) -> Result<UntrustedSpan, ParseError> {
        if self.pending_type_equals {
            return Err(unsupported(self.current(), "unconsumed type initializer"));
        }
        if let Some(token) = self.current().filter(|token| token.kind() == TokenKind::GreaterEqual)
        {
            self.pending_type_equals = true;
            return Ok(UntrustedSpan { end: token.span().end() - 1, ..raw(token) });
        }
        self.take(TokenKind::GreaterThan).map(raw)
    }

    fn push_type(&mut self, value: syntax::RawTypeSyntax) -> Result<u32, ParseError> {
        if self.types.len() >= syntax::MAX_TYPE_NODES_PER_MODULE
            || self.previous_types + self.types.len() >= syntax::MAX_TYPE_NODES_PER_PROJECT
        {
            return Err(resource("type-syntax inventory exceeds its module or project limit"));
        }
        let id = u32::try_from(self.types.len()).expect("bounded type inventory");
        self.types.push(value);
        Ok(id)
    }

    pub(super) fn type_syntax(&mut self) -> Result<u32, ParseError> {
        self.type_at_depth(1)
    }

    pub(super) fn type_at_depth(&mut self, depth: u32) -> Result<u32, ParseError> {
        if depth > syntax::MAX_NESTING_DEPTH {
            return Err(resource("type syntax exceeds the nesting limit"));
        }
        let name = self.name(super::names::Role::Type)?;
        if super::names::type_diversion(&name.text) {
            return Err(unsupported(
                self.tokens.get(self.position - 1).copied(),
                "unsupported type annotation",
            ));
        }
        if name.text == "FixedArray"
            && self.current().is_some_and(|token| token.kind() == TokenKind::LessThan)
        {
            return self.fixed_array_type(name.span, depth);
        }
        let args = self.type_arguments(depth)?;
        let span =
            UntrustedSpan { end: args.as_ref().map_or(name.span.end, |a| a.span.end), ..name.span };
        let kind = if let Some(args) = args {
            if name.text == "function" {
                return Err(unsupported(self.current(), "unsupported type application"));
            }
            let container =
                matches!(name.text.as_str(), "Vec" | "Shared" | "Weak" | "Borrow" | "BorrowMut");
            if container && args.arguments.len() == 1 {
                if !args.comma_spans.is_empty() {
                    return Err(unsupported(
                        self.current(),
                        "container trailing comma is not represented",
                    ));
                }
                let keyword_span = name.span;
                let less_than_span = args.less_than_span;
                let argument = args.arguments[0];
                let greater_than_span = args.greater_than_span;
                match name.text.as_str() {
                    "Vec" => syntax::RawTypeSyntaxKind::Vec {
                        keyword_span,
                        less_than_span,
                        argument,
                        greater_than_span,
                    },
                    "Shared" => syntax::RawTypeSyntaxKind::Shared {
                        keyword_span,
                        less_than_span,
                        argument,
                        greater_than_span,
                    },
                    "Weak" => syntax::RawTypeSyntaxKind::Weak {
                        keyword_span,
                        less_than_span,
                        argument,
                        greater_than_span,
                    },
                    "Borrow" => syntax::RawTypeSyntaxKind::Borrow {
                        keyword_span,
                        less_than_span,
                        argument,
                        greater_than_span,
                    },
                    _ => syntax::RawTypeSyntaxKind::BorrowMut {
                        keyword_span,
                        less_than_span,
                        argument,
                        greater_than_span,
                    },
                }
            } else {
                if container || matches!(name.text.as_str(), "String" | "FixedArray") {
                    return Err(unsupported(
                        self.current(),
                        "unsupported built-in type argument count",
                    ));
                }
                syntax::RawTypeSyntaxKind::Application { name, type_arguments: args }
            }
        } else if name.text == "String" {
            syntax::RawTypeSyntaxKind::String { keyword_span: name.span }
        } else {
            syntax::RawTypeSyntaxKind::Named { name }
        };
        self.push_type(syntax::RawTypeSyntax { span, kind })
    }

    fn fixed_array_type(
        &mut self,
        keyword_span: UntrustedSpan,
        depth: u32,
    ) -> Result<u32, ParseError> {
        let less_than_span = raw(self.take(TokenKind::LessThan)?);
        let element = self.type_at_depth(depth + 1)?;
        let comma_span = raw(self.take(TokenKind::Comma)?);
        let token = self.take(TokenKind::DecimalInteger)?;
        let length_spelling = self.spelling(token).to_owned();
        let length = length_spelling
            .parse::<u32>()
            .ok()
            .filter(|n| *n <= syntax::MAX_FIXED_ARRAY_LENGTH)
            .filter(|_| length_spelling == "0" || !length_spelling.starts_with('0'))
            .ok_or_else(|| resource("fixed-array length must be canonical and at most 1048576"))?;
        let greater_than_span = self.type_greater_than()?;
        self.push_type(syntax::RawTypeSyntax {
            span: UntrustedSpan { end: greater_than_span.end, ..keyword_span },
            kind: syntax::RawTypeSyntaxKind::FixedArray {
                keyword_span,
                less_than_span,
                element,
                comma_span,
                length_span: raw(token),
                length_spelling,
                length,
                greater_than_span,
            },
        })
    }

    pub(super) fn optional_type(&mut self, insertion: u32) -> Result<u32, ParseError> {
        if self.maybe(TokenKind::Colon).is_some() {
            self.type_syntax()
        } else {
            self.push_type(syntax::RawTypeSyntax {
                span: UntrustedSpan { file: self.file, start: insertion, end: insertion },
                kind: syntax::RawTypeSyntaxKind::Missing,
            })
        }
    }
}
