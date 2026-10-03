//! Diagnostic-only malformed-source scan before syntax normalization.

use super::RejectedSource;
use crate::native_lexer::{Keyword, TokenKind};
use zryna_diagnostics::Diagnostic;

impl RejectedSource<'_> {
    pub(super) fn malformed(&self) -> Option<Diagnostic> {
        let mut first = None;
        self.scan_malformed(|diagnostic| {
            if first.is_none() {
                first = diagnostic;
            }
        });
        first
    }

    pub(super) fn scan_malformed(&self, mut emit: impl FnMut(Option<Diagnostic>)) {
        let mut closers = Vec::new();
        for (index, token) in self.tokens.iter().enumerate() {
            let kind = token.kind();
            emit(self.missing_operand(index));
            if kind == TokenKind::Colon
                && index >= 2
                && self.kind(index - 2) == Some(TokenKind::Colon)
                && closers.last().is_some_and(|(closer, _)| *closer == TokenKind::CloseParen)
            {
                emit(self.parse_error(
                    token.span().start(),
                    token.span().end(),
                    1005,
                    "',' expected.",
                ));
            }
            if kind == TokenKind::DecimalInteger
                && self.spelling(index).starts_with('0')
                && self.spelling(index).len() > 1
                && self.spelling(index).bytes().all(|byte| matches!(byte, b'0'..=b'7'))
            {
                let negative = index > 0 && self.kind(index - 1) == Some(TokenKind::Minus);
                let first = if negative { index - 1 } else { index };
                let replacement =
                    format!("{}0o{}", if negative { "-" } else { "" }, &self.spelling(index)[1..]);
                emit(self.parse_error(
                    self.tokens[first].span().start(),
                    token.span().end(),
                    1121,
                    &format!("Octal literals are not allowed. Use the syntax '{replacement}'."),
                ));
            }
            match kind {
                TokenKind::OpenBrace => closers.push((TokenKind::CloseBrace, "'}' expected.")),
                TokenKind::OpenBracket => closers.push((TokenKind::CloseBracket, "']' expected.")),
                TokenKind::OpenParen => closers.push((TokenKind::CloseParen, "')' expected.")),
                TokenKind::CloseBrace | TokenKind::CloseBracket | TokenKind::CloseParen => {
                    if let Some((expected, message)) = closers.pop()
                        && expected != kind
                    {
                        emit(self.parse_error(
                            token.span().start(),
                            token.span().end(),
                            1005,
                            message,
                        ));
                    }
                }
                _ => {}
            }
            if index >= 2
                && matches!(kind, TokenKind::Plus | TokenKind::Minus)
                && self.kind(index - 1) == Some(kind)
                && self.tokens[index - 1].span().end() == token.span().start()
                && self.is_atom(index - 2)
                && self.is_atom(index + 1)
            {
                let next = self.tokens[index + 1];
                emit(self.parse_error(
                    next.span().start(),
                    next.span().end(),
                    1005,
                    "';' expected.",
                ));
            }
            if kind == TokenKind::Keyword(Keyword::Return)
                && index > 0
                && self.is_atom(index - 1)
                && !super::super::has_line_break(
                    self.text,
                    self.tokens[index - 1].span().end(),
                    token.span().start(),
                )
            {
                emit(self.parse_error(
                    token.span().start(),
                    token.span().end(),
                    1005,
                    "';' expected.",
                ));
            }
        }
        if let Some((_, message)) = closers.last() {
            let end = u32::try_from(self.text.len()).expect("bounded source length");
            emit(self.parse_error(end, end, 1005, message));
        }
    }

    fn missing_operand(&self, index: usize) -> Option<Diagnostic> {
        let token = self.tokens[index];
        let kind = token.kind();
        if matches!(kind, TokenKind::Semicolon | TokenKind::CloseBrace | TokenKind::CloseParen)
            && index > 0
            && matches!(
                self.kind(index - 1),
                Some(TokenKind::Equals | TokenKind::Plus | TokenKind::Minus | TokenKind::Asterisk)
            )
            && !(index >= 3
                && self.kind(index - 1) == self.kind(index - 2)
                && matches!(self.kind(index - 1), Some(TokenKind::Plus | TokenKind::Minus))
                && self.tokens[index - 2].span().end() == self.tokens[index - 1].span().start()
                && matches!(
                    self.kind(index - 3),
                    Some(TokenKind::Identifier | TokenKind::DecimalInteger | TokenKind::CloseParen)
                ))
        {
            return self.parse_error(
                token.span().start(),
                token.span().end(),
                1109,
                "Expression expected.",
            );
        }
        if kind == TokenKind::Colon
            && index >= 3
            && self.kind(index - 1) == Some(TokenKind::OpenParen)
            && self.kind(index - 2) == Some(TokenKind::Identifier)
            && self.kind(index - 3) == Some(TokenKind::Keyword(Keyword::Function))
        {
            return self.parse_error(
                token.span().start(),
                token.span().end(),
                1138,
                "Parameter declaration expected.",
            );
        }
        None
    }

    pub(super) fn is_atom(&self, index: usize) -> bool {
        matches!(
            self.kind(index),
            Some(
                TokenKind::Identifier
                    | TokenKind::DecimalInteger
                    | TokenKind::Keyword(Keyword::True | Keyword::False)
            )
        )
    }
}
