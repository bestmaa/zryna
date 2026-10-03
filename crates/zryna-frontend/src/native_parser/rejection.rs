//! Source-bound diagnostic nodes for rejected scalar grammar.
//!
//! This pass never constructs a candidate. It retains complete rejected-node ranges rather than
//! the individual token at which candidate construction stopped.

use zryna_diagnostics::Diagnostic;
use zryna_source::{SourceMap, UntrustedSpan};

use crate::native_lexer::{Keyword, LexedProject, Token, TokenKind};

mod declarations;
mod m3;
mod malformed;

pub(super) fn malformed_file(
    sources: &SourceMap,
    text: &str,
    tokens: &[Token],
    file: u32,
    version: u32,
) -> Option<Diagnostic> {
    RejectedSource { sources, text, tokens: tokens.to_vec(), file, version }.malformed()
}

pub(super) fn diagnostic(
    sources: &SourceMap,
    lexed: &LexedProject,
    original: Diagnostic,
    version: u32,
) -> Diagnostic {
    if !lexed.is_bound_to(sources) || original.code() != "ZRYNA-F2002" {
        return original;
    }
    let Some(span) = original.primary_span() else { return original };
    let Some(source) = sources.source(span.file()) else { return original };
    let Some(file) = lexed.files().iter().find(|file| file.id() == span.file()) else {
        return original;
    };
    let view = RejectedSource {
        sources,
        text: source.text(),
        tokens: file.tokens().collect(),
        file: file.id().index(),
        version,
    };
    view.malformed()
        .or_else(|| {
            if original.message().starts_with("expression depth uses unsupported syntax") {
                None
            } else {
                view.node(span.start(), original.message())
            }
        })
        .unwrap_or(original)
}

pub(super) fn malformed_v2(
    sources: &SourceMap,
    text: &str,
    tokens: &[Token],
    file: u32,
) -> Option<super::recovery::Diagnostics> {
    use zryna_syntax::v2::{RawDiagnosticLocation, RawProviderDiagnostic};
    let view = RejectedSource { sources, text, tokens: tokens.to_vec(), file, version: 2 };
    let mut diagnostics = super::recovery::Diagnostics::default();
    view.scan_malformed(|diagnostic| {
        if let Some(diagnostic) = diagnostic {
            let (code, message) = diagnostic
                .message()
                .strip_prefix("TypeScript parse error ")
                .expect("parse diagnostic prefix")
                .split_once(": ")
                .expect("parse diagnostic code");
            let span = diagnostic.primary_span().expect("parse diagnostic source span");
            diagnostics.add(RawProviderDiagnostic {
                code: code.to_owned(),
                severity: diagnostic.severity(),
                location: RawDiagnosticLocation::Source {
                    span: UntrustedSpan { file, start: span.start(), end: span.end() },
                },
                message: message.to_owned(),
                guidance: "fix the TypeScript parse error before Zryna analysis".to_owned(),
            });
        }
    });
    (!diagnostics.is_empty()).then_some(diagnostics)
}

struct RejectedSource<'a> {
    sources: &'a SourceMap,
    text: &'a str,
    tokens: Vec<Token>,
    file: u32,
    version: u32,
}

impl RejectedSource<'_> {
    fn located(&self, start: u32, end: u32, message: impl Into<String>) -> Option<Diagnostic> {
        let span = self.sources.verify_span(UntrustedSpan { file: self.file, start, end }).ok()?;
        Some(Diagnostic::error_at(
            "ZRYNA-F2002",
            span,
            message,
            format!("use only the documented protocol-v{} bootstrap syntax", self.version),
        ))
    }

    fn unsupported(
        &self,
        first: usize,
        last: usize,
        context: &str,
        kind: &str,
    ) -> Option<Diagnostic> {
        self.located(
            self.tokens.get(first)?.span().start(),
            self.tokens.get(last)?.span().end(),
            format!("{context} uses unsupported syntax '{kind}'"),
        )
    }

    fn parse_error(&self, start: u32, end: u32, code: u32, message: &str) -> Option<Diagnostic> {
        self.located(start, end, format!("TypeScript parse error TS{code}: {message}"))
    }

    fn spelling(&self, index: usize) -> &str {
        let span = self.tokens[index].span();
        &self.text[span.start() as usize..span.end() as usize]
    }

    fn kind(&self, index: usize) -> Option<TokenKind> {
        self.tokens.get(index).map(|token| token.kind())
    }

    fn matching_close(&self, start: usize) -> Option<usize> {
        let mut closers = Vec::new();
        for index in start..self.tokens.len() {
            match self.tokens[index].kind() {
                TokenKind::OpenBrace => closers.push(TokenKind::CloseBrace),
                TokenKind::OpenBracket => closers.push(TokenKind::CloseBracket),
                TokenKind::OpenParen => closers.push(TokenKind::CloseParen),
                TokenKind::CloseBrace | TokenKind::CloseBracket | TokenKind::CloseParen => {
                    if closers.pop() != self.kind(index) {
                        return None;
                    }
                    if closers.is_empty() {
                        return Some(index);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn statement_bounds(&self, index: usize) -> (usize, usize) {
        let first = (0..index)
            .rev()
            .find(|position| {
                matches!(
                    self.kind(*position),
                    Some(TokenKind::Semicolon | TokenKind::OpenBrace | TokenKind::CloseBrace)
                )
            })
            .map_or(0, |position| position + 1);
        let last = (index..self.tokens.len())
            .find(|position| {
                matches!(self.kind(*position), Some(TokenKind::Semicolon | TokenKind::CloseBrace))
            })
            .unwrap_or(self.tokens.len() - 1);
        (first, last)
    }

    fn statement_node(&self, index: usize, first: usize, last: usize) -> Option<Diagnostic> {
        if self.kind(first) == Some(TokenKind::Keyword(Keyword::Return))
            && self.kind(first + 1) == Some(TokenKind::Semicolon)
        {
            return self.unsupported(first, last, "statement", "ReturnStatement");
        }
        if matches!(self.kind(first), Some(TokenKind::Keyword(Keyword::Let | Keyword::Const)))
            && (self.kind(index) == Some(TokenKind::Comma)
                || !self.tokens[first..index].iter().any(|token| token.kind() == TokenKind::Equals)
                    && super::recovery::primitive_kind(self.spelling(index)).is_none())
        {
            return self.unsupported(first, last, "local declaration", "FirstStatement");
        }
        if matches!(self.kind(first), Some(TokenKind::Keyword(Keyword::If | Keyword::While))) {
            return self.unsupported(
                first,
                last,
                "statement",
                if self.kind(first) == Some(TokenKind::Keyword(Keyword::If)) {
                    "IfStatement"
                } else {
                    "WhileStatement"
                },
            );
        }
        None
    }

    fn import_node(&self, first: usize, index: usize) -> Option<Diagnostic> {
        if let Some(import) = (first..=index)
            .find(|position| self.kind(*position) == Some(TokenKind::Keyword(Keyword::Import)))
        {
            let end = (import..self.tokens.len())
                .find(|position| self.kind(*position) == Some(TokenKind::Semicolon))?;
            let late = self.tokens[..import].iter().any(|token| {
                matches!(token.kind(), TokenKind::Keyword(Keyword::Function | Keyword::Interface))
            });
            return self.unsupported(
                import,
                end,
                if !late {
                    "import declaration"
                } else if self.version == 3 {
                    "import after function"
                } else {
                    "import after declaration"
                },
                "ImportDeclaration",
            );
        }
        None
    }

    fn property_node(&self, index: usize) -> Option<Diagnostic> {
        if self.version == 3
            && self.kind(index) == Some(TokenKind::Dot)
            && index > 0
            && self.kind(index - 1) == Some(TokenKind::Identifier)
        {
            let mut end = index - 1;
            while self.kind(end + 1) == Some(TokenKind::Dot)
                && self.kind(end + 2) == Some(TokenKind::Identifier)
            {
                end += 2;
            }
            return self.unsupported(index - 1, end, "expression", "PropertyAccessExpression");
        }
        None
    }

    fn rejected_value(&self, index: usize, original: &str) -> Option<Diagnostic> {
        if original == "noncanonical integer literal"
            && self.kind(index) == Some(TokenKind::DecimalInteger)
        {
            return self.unsupported(index, index, "expression", "FirstLiteralToken");
        }
        if self.kind(index) == Some(TokenKind::OpenBrace)
            && matches!(
                original,
                "unsupported protocol-v4 expression" | "unsupported aggregate construction"
            )
        {
            return self.unsupported(
                index,
                self.matching_close(index)?,
                "expression",
                "ObjectLiteralExpression",
            );
        }
        None
    }

    fn node(&self, offset: u32, original: &str) -> Option<Diagnostic> {
        let index = self.tokens.iter().position(|token| token.span().end() > offset)?;
        if self.version == 4
            && let Some(diagnostic) = self.m3_node(index, original)
        {
            return Some(diagnostic);
        }
        if let Some(diagnostic) =
            self.declaration_node(index, original).or_else(|| self.rejected_value(index, original))
        {
            return Some(diagnostic);
        }
        if let Some(kind) = super::recovery::primitive_kind(self.spelling(index))
            .or_else(|| (self.spelling(index) == "any").then_some("AnyKeyword"))
        {
            return self.unsupported(index, index, &self.annotation_context(index)?, kind);
        }
        let (first, last) = self.statement_bounds(index);
        if self.version == 3
            && let Some(callee) = (first..=index).find(|position| {
                self.kind(*position) == Some(TokenKind::Identifier)
                    && self.kind(*position + 1) == Some(TokenKind::Dot)
                    && self.kind(*position + 2) == Some(TokenKind::Identifier)
                    && self.kind(*position + 3) == Some(TokenKind::OpenParen)
            })
        {
            return self.unsupported(
                callee,
                self.matching_close(callee + 3)?,
                "expression",
                "CallExpression",
            );
        }
        if let Some(diagnostic) = self.statement_node(index, first, last) {
            return Some(diagnostic);
        }
        if let Some(diagnostic) = self.property_node(index) {
            return Some(diagnostic);
        }
        if let Some(open) = (first..=index).rev().find(|position| {
            self.kind(*position) == Some(TokenKind::OpenParen)
                && (*position == first || self.kind(*position - 1) != Some(TokenKind::Identifier))
        }) && let Some(close) = self.matching_close(open)
        {
            if self.kind(close + 1) == Some(TokenKind::OpenParen) {
                return self.unsupported(
                    open,
                    self.matching_close(close + 1)?,
                    "expression",
                    "CallExpression",
                );
            }
            return self.unsupported(open, close, "expression", "ParenthesizedExpression");
        }
        if let Some(operator) = (first..=index).find(|position| {
            matches!(self.kind(*position), Some(TokenKind::Minus | TokenKind::Plus))
                && self.kind(*position) == self.kind(*position + 1)
                && self.tokens[*position].span().end() == self.tokens[*position + 1].span().start()
        }) {
            if operator > first && self.is_atom(operator - 1) {
                return self.unsupported(
                    operator - 1,
                    operator + 1,
                    "expression",
                    "PostfixUnaryExpression",
                );
            }
            return self.unsupported(operator, operator + 2, "expression", "PrefixUnaryExpression");
        }
        if let Some(diagnostic) = self.import_node(first, index) {
            return Some(diagnostic);
        }
        if let Some(class) = (first..=index).find(|position| self.spelling(*position) == "class") {
            let open = (class..self.tokens.len())
                .find(|position| self.kind(*position) == Some(TokenKind::OpenBrace))?;
            return self.unsupported(
                first,
                self.matching_close(open)?,
                "top-level declaration",
                "ClassDeclaration",
            );
        }
        if (self.kind(index) == Some(TokenKind::Equals)
            || self.kind(index + 1) == Some(TokenKind::Equals))
            && index > first
        {
            let end = if self.kind(last) == Some(TokenKind::Semicolon) { last - 1 } else { last };
            return self.unsupported(
                first,
                if self.version == 3 { last } else { end },
                if self.version == 3 { "statement" } else { "expression" },
                if self.version == 3 { "ExpressionStatement" } else { "BinaryExpression" },
            );
        }
        if self.kind(index) == Some(TokenKind::StringLiteral) && self.version == 3 {
            return self.unsupported(index, index, "expression", "StringLiteral");
        }
        None
    }
}
