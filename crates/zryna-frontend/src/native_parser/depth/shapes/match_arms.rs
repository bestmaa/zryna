//! Match barriers retain expressions normalized before an invalid arm.

use super::{Node, expression, spelling, take};
use crate::native_lexer::{Token, TokenKind};

pub(super) fn object_end(tokens: &[Token], start: usize) -> Option<(usize, usize)> {
    super::super::super::collections::bounds(tokens, start)
}

fn name(value: &str) -> bool {
    let mut bytes = value.bytes();
    value.len() <= 128
        && !matches!(value, "constructor" | "prototype" | "__proto__")
        && bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

pub(super) fn parse(
    tokens: &[Token],
    position: &mut usize,
    text: &str,
    nodes: &mut Vec<Node>,
    nesting: u32,
) -> Option<(Vec<u32>, bool)> {
    if tokens.get(*position)?.kind() != TokenKind::OpenBrace {
        return None;
    }
    let (end, count) = object_end(tokens, *position)?;
    let mut children = Vec::new();
    if count > 1024 {
        *position = end;
        return Some((children, true));
    }
    let mut keys = std::collections::BTreeSet::new();
    let parsed = (|| {
        *position += 1;
        while tokens.get(*position)?.kind() != TokenKind::CloseBrace {
            let key = spelling(text, take(tokens, position, TokenKind::StringLiteral)?);
            let qualified = key.strip_prefix('"')?.strip_suffix('"')?;
            let (owner, variant) = qualified.split_once('.')?;
            if !name(owner) || !name(variant) || !keys.insert(qualified) {
                return None;
            }
            take(tokens, position, TokenKind::Colon)?;
            take(tokens, position, TokenKind::OpenParen)?;
            if tokens.get(*position)?.kind() == TokenKind::Identifier {
                if !name(spelling(text, tokens[*position])) {
                    return None;
                }
                *position += 1;
            }
            take(tokens, position, TokenKind::CloseParen)?;
            take(tokens, position, TokenKind::FatArrow)?;
            children.push(expression(tokens, position, text, nodes, 4, nesting + 1)?);
            if take(tokens, position, TokenKind::Comma).is_none() {
                break;
            }
        }
        take(tokens, position, TokenKind::CloseBrace)?;
        Some(())
    })();
    *position = end;
    Some((children, parsed.is_none()))
}
