//! V5 collection counts retain generic commas across source-admitted keyword type heads.
//!
//! The inherited v4 helper remains protected. Identifier roles and type syntax are still
//! authenticated by the parser and independent v5 source verifier; counting grants no semantics.

use crate::native_lexer::{Token, TokenKind};

pub(super) fn bounds(tokens: &[Token], start: usize) -> Option<(usize, usize)> {
    separated_bounds(tokens, start, TokenKind::Comma, false)
}

pub(super) fn separated_bounds(
    tokens: &[Token],
    start: usize,
    separator: TokenKind,
    types: bool,
) -> Option<(usize, usize)> {
    let mut closers = Vec::new();
    let mut count = 0;
    let mut first = true;
    let mut generic = Vec::new();
    let mut index = start;
    while let Some(token) = tokens.get(index) {
        if closers.len() == 1 && Some(token.kind()) != closers.last().copied() {
            if first {
                count += 1;
                first = false;
            }
            if token.kind() == separator
                && tokens
                    .get(index + 1)
                    .is_some_and(|next| Some(next.kind()) != closers.last().copied())
            {
                count += 1;
            }
        }
        // Restore commas inside generic construction types once the call is known.
        match token.kind() {
            TokenKind::LessThan => generic.push(count),
            TokenKind::GreaterThan => {
                if let Some(before) = generic.pop()
                    && (types
                        || tokens
                            .get(index + 1)
                            .is_some_and(|next| next.kind() == TokenKind::OpenParen))
                {
                    count = before;
                }
            }
            TokenKind::Comma
            | TokenKind::Identifier
            | TokenKind::Keyword(_)
            | TokenKind::DecimalInteger => {}
            _ => generic.clear(),
        }
        match token.kind() {
            TokenKind::OpenBrace => closers.push(TokenKind::CloseBrace),
            TokenKind::OpenParen => closers.push(TokenKind::CloseParen),
            TokenKind::OpenBracket => closers.push(TokenKind::CloseBracket),
            TokenKind::CloseBrace | TokenKind::CloseParen | TokenKind::CloseBracket => {
                if closers.pop() != Some(token.kind()) {
                    return None;
                }
                if closers.is_empty() {
                    return Some((index + 1, count));
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

pub(super) fn arguments(tokens: &[Token], open: usize) -> Option<Vec<(usize, usize)>> {
    ranges(tokens, open, false)
}

fn ranges(tokens: &[Token], open: usize, types: bool) -> Option<Vec<(usize, usize)>> {
    let mut closers = Vec::new();
    let mut ranges = Vec::new();
    let mut generic = Vec::new();
    let mut start = open + 1;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if closers.len() == 1 {
            match token.kind() {
                TokenKind::LessThan => generic.push((ranges.len(), start)),
                TokenKind::GreaterThan => {
                    if let Some((length, first)) = generic.pop()
                        && (types
                            || tokens
                                .get(index + 1)
                                .is_some_and(|next| next.kind() == TokenKind::OpenParen))
                    {
                        ranges.truncate(length);
                        start = first;
                    }
                }
                TokenKind::Comma => {
                    ranges.push((start, index));
                    start = index + 1;
                }
                TokenKind::Identifier | TokenKind::Keyword(_) | TokenKind::DecimalInteger => {}
                _ => generic.clear(),
            }
        }
        match token.kind() {
            TokenKind::OpenParen => closers.push(TokenKind::CloseParen),
            TokenKind::OpenBrace => closers.push(TokenKind::CloseBrace),
            TokenKind::OpenBracket => closers.push(TokenKind::CloseBracket),
            TokenKind::CloseParen | TokenKind::CloseBrace | TokenKind::CloseBracket => {
                if closers.pop() != Some(token.kind()) {
                    return None;
                }
                if closers.is_empty() {
                    if start < index {
                        ranges.push((start, index));
                    }
                    return Some(ranges);
                }
            }
            _ => {}
        }
    }
    None
}
