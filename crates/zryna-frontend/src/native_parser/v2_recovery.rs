//! Source-node recovery for rejected protocol-v2 declarations and signatures.

use super::{AnnotationContext, FileParser, ParseError};
use crate::native_lexer::{Keyword, Token, TokenKind};
use zryna_source::UntrustedSpan;
use zryna_syntax::v2 as syntax;

impl FileParser<'_> {
    pub(super) fn recover_complex_annotation(
        &mut self,
        context: &AnnotationContext,
    ) -> Result<Option<syntax::RawTypeSyntax>, ParseError> {
        if !self.recovering {
            return Ok(None);
        }
        let start = self.position;
        let first = self.current().expect("annotation starts at a token");
        let mut end = start;
        while self.tokens.get(end + 1).is_some_and(|token| token.kind() == TokenKind::Dot)
            && self.tokens.get(end + 2).is_some_and(|token| token.kind() == TokenKind::Identifier)
        {
            end += 2;
        }
        if self.tokens.get(end + 1).is_some_and(|token| token.kind() == TokenKind::LessThan) {
            end += 1;
            let mut depth = 0;
            loop {
                let Some(token) = self.tokens.get(end) else {
                    return Err(self.error_here("ZRYNA-F2002", "incomplete type arguments"));
                };
                match token.kind() {
                    TokenKind::LessThan => depth += 1,
                    TokenKind::GreaterThan => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    break;
                }
                end += 1;
            }
        }
        let mut array = false;
        while self.tokens.get(end + 1).is_some_and(|token| token.kind() == TokenKind::OpenBracket)
            && self.tokens.get(end + 2).is_some_and(|token| token.kind() == TokenKind::CloseBracket)
        {
            end += 2;
            array = true;
        }
        if end == start {
            return Ok(None);
        }
        let label = match context {
            AnnotationContext::Parameter(index) => format!("parameter {index} annotation"),
            AnnotationContext::Result(index) => format!("function {index} result annotation"),
        };
        let kind = if array { "ArrayType" } else { "TypeReference" };
        let last = self.tokens[end];
        let error =
            self.error_between(first, last, format!("{label} uses unsupported syntax '{kind}'"));
        self.retain_error(&error);
        self.position = end + 1;
        Ok(Some(syntax::RawTypeSyntax {
            span: UntrustedSpan {
                file: self.file,
                start: first.span().start(),
                end: last.span().end(),
            },
            kind: syntax::RawTypeSyntaxKind::Named { name: self.spelling(first).to_owned() },
        }))
    }

    pub(super) fn recover_parameter_initializer(&mut self, index: usize) -> Result<(), ParseError> {
        if !self.recovering || self.maybe(TokenKind::Equals).is_none() {
            return Ok(());
        }
        let start = self.position;
        let first =
            self.current().ok_or_else(|| self.error_here("ZRYNA-F2002", "missing initializer"))?;
        let mut closers = Vec::new();
        while let Some(token) = self.current() {
            if closers.is_empty()
                && matches!(token.kind(), TokenKind::Comma | TokenKind::CloseParen)
            {
                break;
            }
            match token.kind() {
                TokenKind::OpenParen => closers.push(TokenKind::CloseParen),
                TokenKind::OpenBracket => closers.push(TokenKind::CloseBracket),
                TokenKind::OpenBrace => closers.push(TokenKind::CloseBrace),
                TokenKind::CloseParen | TokenKind::CloseBracket | TokenKind::CloseBrace
                    if closers.pop() != Some(token.kind()) =>
                {
                    return Err(self.error_here("ZRYNA-F2002", "mismatched initializer delimiter"));
                }
                _ => {}
            }
            self.position += 1;
        }
        if self.position == start {
            return Err(self.error_here("ZRYNA-F2002", "missing initializer"));
        }
        let kind =
            self.initializer_kind(start, self.position - 1).unwrap_or_else(|| match first.kind() {
                TokenKind::DecimalInteger => "FirstLiteralToken",
                TokenKind::StringLiteral => "StringLiteral",
                TokenKind::Minus | TokenKind::Plus => "PrefixUnaryExpression",
                TokenKind::OpenParen => "ParenthesizedExpression",
                TokenKind::OpenBrace => "ObjectLiteralExpression",
                TokenKind::OpenBracket => "ArrayLiteralExpression",
                TokenKind::Keyword(Keyword::True) => "TrueKeyword",
                TokenKind::Keyword(Keyword::False) => "FalseKeyword",
                TokenKind::Identifier
                    if self
                        .tokens
                        .get(start + 1)
                        .is_some_and(|token| token.kind() == TokenKind::OpenParen) =>
                {
                    "CallExpression"
                }
                _ => "Identifier",
            });
        let error = self.error_between(
            first,
            self.tokens[self.position - 1],
            format!("parameter {index} uses unsupported syntax '{kind}'"),
        );
        self.retain_error(&error);
        Ok(())
    }

    fn node_end(&self, start: usize, block: bool) -> Option<usize> {
        let mut closers = Vec::new();
        for (index, token) in self.tokens.iter().enumerate().skip(start) {
            match token.kind() {
                TokenKind::OpenParen => closers.push(TokenKind::CloseParen),
                TokenKind::OpenBracket => closers.push(TokenKind::CloseBracket),
                TokenKind::OpenBrace => closers.push(TokenKind::CloseBrace),
                TokenKind::CloseParen | TokenKind::CloseBracket | TokenKind::CloseBrace => {
                    if closers.pop() != Some(token.kind()) {
                        return None;
                    }
                    if block && closers.is_empty() && token.kind() == TokenKind::CloseBrace {
                        return Some(index);
                    }
                }
                TokenKind::Semicolon if closers.is_empty() => return Some(index),
                _ => {}
            }
        }
        None
    }

    fn initializer_kind(&self, start: usize, end: usize) -> Option<&'static str> {
        let mut depth = 0;
        let mut property = false;
        for index in start..=end {
            let kind = self.tokens[index].kind();
            match kind {
                TokenKind::OpenParen | TokenKind::OpenBrace | TokenKind::OpenBracket => depth += 1,
                TokenKind::CloseParen | TokenKind::CloseBrace | TokenKind::CloseBracket => {
                    depth -= 1;
                }
                TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Asterisk
                | TokenKind::Equals
                | TokenKind::LessThan
                | TokenKind::GreaterThan
                | TokenKind::StrictEqual
                | TokenKind::LessEqual
                | TokenKind::GreaterEqual
                    if depth == 0 && index > start =>
                {
                    if !matches!(self.tokens[index - 1].kind(), TokenKind::Plus | TokenKind::Minus)
                    {
                        return Some("BinaryExpression");
                    }
                }
                TokenKind::Dot if depth == 0 => property = true,
                _ => {}
            }
        }
        if property {
            Some(if self.tokens[end].kind() == TokenKind::CloseParen {
                "CallExpression"
            } else {
                "PropertyAccessExpression"
            })
        } else {
            None
        }
    }

    pub(super) fn recover_top_level(&mut self) -> bool {
        let start = self.position;
        let Some(first) = self.current() else { return false };
        let head =
            if first.kind() == TokenKind::Keyword(Keyword::Export) { start + 1 } else { start };
        let Some(token) = self.tokens.get(head).copied() else { return false };
        let (kind, block) = match token.kind() {
            TokenKind::Keyword(Keyword::Const | Keyword::Let) => ("FirstStatement", false),
            TokenKind::Keyword(Keyword::Interface) => ("InterfaceDeclaration", true),
            TokenKind::Keyword(Keyword::Import) => ("ImportDeclaration", false),
            TokenKind::Identifier if self.spelling(token) == "class" => ("ClassDeclaration", true),
            _ => return false,
        };
        let Some(end) = self.node_end(start, block) else { return false };
        let error = self.error_between(
            first,
            self.tokens[end],
            format!("top-level declaration uses unsupported syntax '{kind}'"),
        );
        self.retain_error(&error);
        self.position = end + 1;
        true
    }

    pub(super) fn function_prefix(&mut self, index: usize) -> Result<(Token, Token), ParseError> {
        let export = self.maybe(TokenKind::Keyword(Keyword::Export));
        if !self.recovering && export.is_none() {
            return Err(self.error_here("ZRYNA-F2002", "unsupported token in native syntax slice"));
        }
        let keyword = self.take(TokenKind::Keyword(Keyword::Function))?;
        if export.is_none() {
            let end = self
                .node_end(self.position - 1, true)
                .ok_or_else(|| self.error_here("ZRYNA-F2002", "incomplete function"))?;
            let error = self.error_between(
                keyword,
                self.tokens[end],
                format!("top-level function {index} uses unsupported syntax 'FunctionDeclaration'"),
            );
            self.retain_error(&error);
        }
        if self.recovering
            && let Some(star) = self.maybe(TokenKind::Asterisk)
        {
            let error = self.error_between(
                star,
                star,
                format!("function {index} generator uses unsupported syntax 'AsteriskToken'"),
            );
            self.retain_error(&error);
        }
        Ok((export.unwrap_or(keyword), keyword))
    }

    pub(super) fn recover_type_parameters(&mut self, index: usize) -> Result<(), ParseError> {
        if !self.recovering || self.maybe(TokenKind::LessThan).is_none() {
            return Ok(());
        }
        let mut start = self.position;
        let mut depth = 1;
        while let Some(token) = self.current() {
            match token.kind() {
                TokenKind::LessThan => depth += 1,
                TokenKind::GreaterThan => depth -= 1,
                _ => {}
            }
            if depth == 0 || (depth == 1 && token.kind() == TokenKind::Comma) {
                if self.position > start {
                    let error = self.error_between(self.tokens[start], self.tokens[self.position - 1],
                        format!("function {index} type parameter uses unsupported syntax 'TypeParameter'"));
                    self.retain_error(&error);
                }
                start = self.position + 1;
            }
            self.position += 1;
            if depth == 0 {
                return Ok(());
            }
        }
        Err(self.error_here("ZRYNA-F2002", "incomplete function type parameters"))
    }

    pub(super) fn recover_statement(&mut self) -> bool {
        let Some(first) = self.current() else { return false };
        let (kind, block) = match first.kind() {
            TokenKind::OpenBrace => ("Block", true),
            TokenKind::Keyword(Keyword::Const | Keyword::Let) => ("FirstStatement", false),
            TokenKind::Keyword(Keyword::If) => ("IfStatement", true),
            TokenKind::Keyword(Keyword::While) => ("WhileStatement", true),
            _ => return false,
        };
        let Some(mut end) = self.node_end(self.position, block) else { return false };
        while first.kind() == TokenKind::Keyword(Keyword::If)
            && self
                .tokens
                .get(end + 1)
                .is_some_and(|token| token.kind() == TokenKind::Keyword(Keyword::Else))
        {
            let Some(else_end) = self.node_end(end + 2, true) else { return false };
            end = else_end;
        }
        let error = self.error_between(
            first,
            self.tokens[end],
            format!("statement uses unsupported syntax '{kind}'"),
        );
        self.retain_error(&error);
        self.position = end + 1;
        true
    }
}
