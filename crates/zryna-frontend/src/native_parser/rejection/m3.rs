//! Complete diagnostic nodes for rejected ownership and callback syntax.

use super::RejectedSource;
use crate::native_lexer::TokenKind;
use zryna_diagnostics::Diagnostic;

impl RejectedSource<'_> {
    fn enclosed_call(&self, index: usize, name: &str) -> Option<(usize, usize)> {
        (0..=index).rev().find_map(|callee| {
            if self.kind(callee) != Some(TokenKind::Identifier)
                || self.spelling(callee) != name
                || self.kind(callee + 1) != Some(TokenKind::OpenParen)
            {
                return None;
            }
            let close = self.matching_close(callee + 1)?;
            (index <= close).then_some((callee, close))
        })
    }

    fn entries(&self, open: usize) -> Option<Vec<(usize, usize)>> {
        Some(
            super::super::collections::arguments(&self.tokens, open)?
                .into_iter()
                .filter(|(start, end)| start < end)
                .map(|(start, end)| (start, end - 1))
                .collect(),
        )
    }

    fn valid_callback(&self, range: (usize, usize), success: bool) -> bool {
        let (start, end) = range;
        if self.kind(start) != Some(TokenKind::OpenParen) {
            return false;
        }
        let Some(close) = self.matching_close(start) else { return false };
        let parameters = if success {
            close == start + 2 && self.kind(start + 1) == Some(TokenKind::Identifier)
        } else {
            close == start + 1
        };
        parameters
            && self.kind(close + 1) == Some(TokenKind::FatArrow)
            && self.kind(close + 2) == Some(TokenKind::OpenBrace)
            && self.matching_close(close + 2) == Some(end)
    }

    fn arrow_callback(&self, (start, end): (usize, usize)) -> bool {
        let mut position = start;
        while position <= end {
            if self.kind(position) == Some(TokenKind::FatArrow) {
                return true;
            }
            if matches!(
                self.kind(position),
                Some(TokenKind::OpenParen | TokenKind::OpenBracket | TokenKind::OpenBrace)
            ) {
                let Some(close) = self.matching_close(position) else { return false };
                position = close + 1;
            } else {
                position += 1;
            }
        }
        false
    }

    pub(super) fn m3_node(&self, index: usize, original: &str) -> Option<Diagnostic> {
        if original == "unsupported typed construction"
            && self.kind(index + 1) == Some(TokenKind::LessThan)
        {
            let open = super::super::collections::generic_end(&self.tokens, index + 1)?;
            if self.kind(open) == Some(TokenKind::OpenParen) {
                let count =
                    super::super::collections::generic_arguments(&self.tokens, index + 1, open);
                let arguments = self.entries(open)?;
                let arity = if self.spelling(index) == "Vec" { 1 } else { 2 };
                return self.unsupported(
                    index,
                    self.matching_close(open)?,
                    if arguments.len() == 1 && count != arity {
                        "typed array construction"
                    } else {
                        "typed construction"
                    },
                    "CallExpression",
                );
            }
        }
        if original == "wrong intrinsic argument count"
            && let Some((callee, close)) = self.enclosed_call(index, "push")
        {
            return self.unsupported(callee, close, "vector push", "CallExpression");
        }
        if let Some((callee, close)) = self.enclosed_call(index, "upgradeWeak") {
            let arguments = self.entries(callee + 1)?;
            if arguments.len() != 3
                || !self.arrow_callback(arguments[1])
                || !self.arrow_callback(arguments[2])
            {
                return self.unsupported(callee, close, "weak upgrade", "CallExpression");
            }
            if !self.valid_callback(arguments[1], true) || !self.valid_callback(arguments[2], false)
            {
                return self.unsupported(callee, close, "weak upgrade callbacks", "CallExpression");
            }
        }
        if let Some((callee, _)) = self.enclosed_call(index, "match") {
            let arguments = self.entries(callee + 1)?;
            if arguments.len() != 2 {
                return self.unsupported(
                    callee,
                    self.matching_close(callee + 1)?,
                    "match expression",
                    "CallExpression",
                );
            }
            if arguments.len() == 2 && self.kind(arguments[1].0) == Some(TokenKind::OpenBrace) {
                let property = self
                    .entries(arguments[1].0)?
                    .into_iter()
                    .find(|(start, end)| *start <= index && index <= *end)?;
                return self.match_property(property, original);
            }
        }
        for callee in (0..=index).rev() {
            if self.kind(callee) == Some(TokenKind::Identifier)
                && self.kind(callee + 1) == Some(TokenKind::Dot)
                && self.kind(callee + 2) == Some(TokenKind::Identifier)
                && self.kind(callee + 3) == Some(TokenKind::OpenParen)
            {
                let close = self.matching_close(callee + 3)?;
                if index <= close && self.entries(callee + 3)?.len() > 1 {
                    return self.unsupported(callee, close, "expression", "CallExpression");
                }
            }
        }
        None
    }

    fn match_property(&self, (key, end): (usize, usize), original: &str) -> Option<Diagnostic> {
        if original == "unsupported match arm key" {
            return self.unsupported(key, key, "match arm key", "StringLiteral");
        }
        if self.kind(key) != Some(TokenKind::StringLiteral) {
            return self.unsupported(key, end, "match arm", "PropertyAssignment");
        }
        let start = key + 2;
        if !self.arrow_callback((start, end)) {
            return self.unsupported(key, end, "match arm", "PropertyAssignment");
        }
        if self.kind(start) != Some(TokenKind::OpenParen) {
            return self.unsupported(start, end, "match arm function", "ArrowFunction");
        }
        let close = self.matching_close(start)?;
        let parameters = self.entries(start)?;
        if parameters.len() > 1
            || self.kind(close + 1) != Some(TokenKind::FatArrow)
            || self.kind(close + 2) == Some(TokenKind::OpenBrace)
        {
            return self.unsupported(start, end, "match arm function", "ArrowFunction");
        }
        if let Some((first, last)) = parameters.first()
            && (first != last || self.kind(*first) != Some(TokenKind::Identifier))
        {
            return self.unsupported(*first, *last, "match arm binding", "Parameter");
        }
        None
    }
}
