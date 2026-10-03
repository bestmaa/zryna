//! Complete rejected import, declaration, type, and intrinsic-call nodes.

use super::RejectedSource;
use crate::native_lexer::{Keyword, TokenKind};
use zryna_diagnostics::Diagnostic;

impl RejectedSource<'_> {
    fn array_element_annotation(&self, index: usize) -> bool {
        (1..index).rev().any(|less| {
            if self.kind(less) != Some(TokenKind::LessThan)
                || !matches!(self.spelling(less - 1), "Vec" | "FixedArray")
            {
                return false;
            }
            let Some(end) = super::super::collections::generic_end(&self.tokens, less) else {
                return false;
            };
            index < end
                && self.kind(end) == Some(TokenKind::OpenParen)
                && self.kind(end + 1) == Some(TokenKind::OpenBracket)
        })
    }

    pub(super) fn annotation_context(&self, index: usize) -> Option<String> {
        if self.array_element_annotation(index) {
            return Some("array element type".to_owned());
        }
        if let Some(interface) = (0..index)
            .rev()
            .find(|position| self.kind(*position) == Some(TokenKind::Keyword(Keyword::Interface)))
        {
            let open = (interface..index)
                .find(|position| self.kind(*position) == Some(TokenKind::OpenBrace));
            if open.is_some_and(|open| self.matching_close(open).is_some_and(|close| index < close))
            {
                return Some("data member type".to_owned());
            }
        }
        let (first, _) = self.statement_bounds(index);
        if matches!(self.kind(first), Some(TokenKind::Keyword(Keyword::Let | Keyword::Const))) {
            return Some("local annotation".to_owned());
        }
        let function = (0..index)
            .rev()
            .find(|position| self.kind(*position) == Some(TokenKind::Keyword(Keyword::Function)))?;
        let open = (function..index)
            .find(|position| self.kind(*position) == Some(TokenKind::OpenParen))?;
        if index > self.matching_close(open)? {
            let ordinal = self.tokens[..function]
                .iter()
                .filter(|token| token.kind() == TokenKind::Keyword(Keyword::Function))
                .count();
            return Some(format!("function {ordinal} result annotation"));
        }
        let ordinal = super::super::collections::parameters(&self.tokens, open)?
            .iter()
            .position(|(start, end)| *start <= index && index < *end)?;
        Some(format!("parameter {ordinal} annotation"))
    }

    fn data_node(&self, index: usize, last: usize, original: &str) -> Option<Diagnostic> {
        if let Some(interface) = (0..=index)
            .rev()
            .find(|position| self.kind(*position) == Some(TokenKind::Keyword(Keyword::Interface)))
        {
            let open = (interface..self.tokens.len())
                .find(|position| self.kind(*position) == Some(TokenKind::OpenBrace))?;
            let close = self.matching_close(open)?;
            if index <= close {
                if original == "unsupported data-declaration marker" {
                    let marker = (interface..open).find(|position| {
                        self.kind(*position) == Some(TokenKind::Keyword(Keyword::Extends))
                    })? + 1;
                    return self.unsupported(
                        marker,
                        marker,
                        "data declaration marker",
                        "ExpressionWithTypeArguments",
                    );
                }
                if original == "empty data declaration" {
                    let start = if interface > 0
                        && self.kind(interface - 1) == Some(TokenKind::Keyword(Keyword::Export))
                    {
                        interface - 1
                    } else {
                        interface
                    };
                    return self.unsupported(
                        start,
                        close,
                        "empty data declaration",
                        "InterfaceDeclaration",
                    );
                }
                if original == "duplicate data member" {
                    return self.unsupported(
                        index - 1,
                        index - 1,
                        "duplicate or invalid data member name",
                        "Identifier",
                    );
                }
                if let Some(kind) = super::super::recovery::primitive_kind(self.spelling(index))
                    .or_else(|| (self.spelling(index) == "any").then_some("AnyKeyword"))
                {
                    return self.unsupported(index, index, "data member type", kind);
                }
                if self.kind(index) == Some(TokenKind::OpenParen) {
                    return self.unsupported(index - 1, last, "data member", "MethodSignature");
                }
            }
        }
        None
    }

    pub(super) fn declaration_node(&self, index: usize, original: &str) -> Option<Diagnostic> {
        let (first, last) = self.statement_bounds(index);
        if original.contains("empty named import") || original.contains("must contain a binding") {
            return self.unsupported(index - 1, index, "empty named import", "NamedImports");
        }
        if original == "unsupported module specifier" {
            return self.unsupported(index, index, "module specifier", "StringLiteral");
        }
        if self.kind(first) == Some(TokenKind::Keyword(Keyword::Export))
            && matches!(
                self.kind(first + 1),
                Some(TokenKind::Keyword(Keyword::Let | Keyword::Const))
            )
        {
            return self.unsupported(first, last, "top-level declaration", "FirstStatement");
        }
        if original == "wrong intrinsic argument count" {
            let callee = (0..index).rev().find(|position| {
                self.kind(*position) == Some(TokenKind::Identifier)
                    && self.kind(*position + 1) == Some(TokenKind::OpenParen)
            })?;
            return self.unsupported(
                callee,
                self.matching_close(callee + 1)?,
                "ownership intrinsic",
                "CallExpression",
            );
        }
        if original == "duplicate struct initializer" {
            return self.unsupported(
                index - 1,
                index - 1,
                "duplicate aggregate construction field",
                "Identifier",
            );
        }
        if let Some(diagnostic) = self.data_node(index, last, original) {
            return Some(diagnostic);
        }
        if let Some(function) = (0..=index)
            .rev()
            .find(|position| self.kind(*position) == Some(TokenKind::Keyword(Keyword::Function)))
        {
            let open = (function..self.tokens.len())
                .find(|position| self.kind(*position) == Some(TokenKind::OpenParen))?;
            if index < open {
                let body = (open..self.tokens.len())
                    .find(|position| self.kind(*position) == Some(TokenKind::OpenBrace))?;
                let ordinal = self.tokens[..function]
                    .iter()
                    .filter(|token| token.kind() == TokenKind::Keyword(Keyword::Function))
                    .count();
                let start = if function > 0
                    && self.kind(function - 1) == Some(TokenKind::Keyword(Keyword::Export))
                {
                    function - 1
                } else {
                    function
                };
                return self.unsupported(
                    start,
                    self.matching_close(body)?,
                    &format!("function {ordinal}"),
                    "FunctionDeclaration",
                );
            }
        }
        let less = if original == "unsupported type argument count" {
            (self.kind(index + 1) == Some(TokenKind::LessThan)).then_some(index + 1)
        } else {
            (first..=index)
                .rev()
                .find(|position| self.kind(*position) == Some(TokenKind::LessThan))
                .or_else(|| {
                    (self.kind(index + 1) == Some(TokenKind::LessThan)).then_some(index + 1)
                })
        };
        if let Some(less) = less {
            let mut depth = 0;
            for end in less..self.tokens.len() {
                if self.kind(end) == Some(TokenKind::LessThan) {
                    depth += 1;
                }
                if matches!(self.kind(end), Some(TokenKind::GreaterThan | TokenKind::GreaterEqual))
                {
                    depth -= 1;
                    if depth == 0 && index <= end {
                        return self.located(
                            self.tokens[less.checked_sub(1)?].span().start(),
                            self.tokens[end].span().end()
                                - u32::from(self.kind(end) == Some(TokenKind::GreaterEqual)),
                            format!(
                                "{} uses unsupported syntax 'TypeReference'",
                                self.annotation_context(index)?
                            ),
                        );
                    }
                }
            }
        }
        None
    }
}
