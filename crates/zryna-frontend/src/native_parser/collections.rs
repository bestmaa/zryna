//! Source collection counts before any element or field normalization.

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
            TokenKind::Comma | TokenKind::Identifier | TokenKind::DecimalInteger => {}
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

pub(super) fn generic_arguments(tokens: &[Token], start: usize, end: usize) -> usize {
    let mut depth = 0;
    let mut arguments = 1;
    for token in &tokens[start..end] {
        match token.kind() {
            TokenKind::LessThan => depth += 1,
            TokenKind::GreaterThan | TokenKind::GreaterEqual => depth -= 1,
            TokenKind::Comma if depth == 1 => arguments += 1,
            _ => {}
        }
    }
    arguments
}

pub(super) fn generic_end(tokens: &[Token], start: usize) -> Option<usize> {
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        match token.kind() {
            TokenKind::LessThan => depth += 1,
            TokenKind::GreaterThan | TokenKind::GreaterEqual => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            TokenKind::Semicolon | TokenKind::CloseBrace => return None,
            _ => {}
        }
    }
    None
}

pub(super) fn arguments(tokens: &[Token], open: usize) -> Option<Vec<(usize, usize)>> {
    ranges(tokens, open, false)
}

pub(super) fn parameters(tokens: &[Token], open: usize) -> Option<Vec<(usize, usize)>> {
    ranges(tokens, open, true)
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
                TokenKind::Identifier | TokenKind::DecimalInteger => {}
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
