//! Complete call and construction ranges for diagnostic-only expression shapes.

use super::{Node, expression, push, spelling, take};
use crate::native_lexer::{Token, TokenKind};
use zryna_source::UntrustedSpan;

pub(super) fn starts(tokens: &[Token], position: usize) -> bool {
    if tokens.get(position).is_some_and(|token| token.kind() == TokenKind::OpenParen) {
        return true;
    }
    if !tokens.get(position).is_some_and(|token| token.kind() == TokenKind::LessThan) {
        return false;
    }
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate().skip(position) {
        match token.kind() {
            TokenKind::LessThan => depth += 1,
            TokenKind::GreaterThan => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return tokens.get(index + 1).is_some_and(|token| token.kind() == TokenKind::OpenParen);
        }
        if matches!(token.kind(), TokenKind::Semicolon | TokenKind::CloseBrace) {
            return false;
        }
    }
    false
}

fn type_arguments(tokens: &[Token], position: &mut usize, text: &str) -> Option<(usize, bool)> {
    if !tokens.get(*position).is_some_and(|token| token.kind() == TokenKind::LessThan) {
        return Some((0, false));
    }
    let mut depth = 0;
    let mut count = 1;
    let mut stop = false;
    let mut forms = Vec::new();
    while let Some(token) = tokens.get(*position) {
        match token.kind() {
            TokenKind::LessThan => {
                depth += 1;
                stop |= depth >= 128;
                let name = spelling(text, *tokens.get(position.checked_sub(1)?)?);
                forms.push((name, 1));
            }
            TokenKind::GreaterThan => {
                depth -= 1;
                let (form, arguments) = forms.pop()?;
                stop |= match form {
                    "Vec" | "Shared" | "Weak" | "Borrow" | "BorrowMut" => arguments != 1,
                    "FixedArray" => arguments != 2,
                    _ => true,
                };
            }
            TokenKind::Comma => {
                if depth == 1 {
                    count += 1;
                }
                forms.last_mut()?.1 += 1;
            }
            TokenKind::Identifier => {
                stop |= super::super::super::recovery::primitive_kind(spelling(text, *token))
                    .is_some()
                    || matches!(
                        spelling(text, *token),
                        "any" | "keyof" | "readonly" | "unique" | "infer"
                    );
            }
            TokenKind::DecimalInteger => {
                let literal = spelling(text, *token);
                stop |= forms
                    .last()
                    .is_none_or(|(form, argument)| *form != "FixedArray" || *argument != 2)
                    || literal.len() > 10
                    || literal != "0" && literal.starts_with('0')
                    || !literal.parse::<u32>().is_ok_and(|length| length <= 1_048_576);
            }
            _ => stop = true,
        }
        *position += 1;
        if depth == 0 {
            return Some((count, stop));
        }
    }
    None
}

fn collection(
    tokens: &[Token],
    position: &mut usize,
    text: &str,
    nodes: &mut Vec<Node>,
    version: u32,
    nesting: u32,
    object: bool,
) -> Option<(Vec<u32>, bool, bool)> {
    *position += 1;
    let close = if object { TokenKind::CloseBrace } else { TokenKind::CloseBracket };
    let mut children = Vec::new();
    let mut keys = std::collections::BTreeSet::new();
    let mut stop = false;
    let mut after_stop = false;
    while tokens.get(*position)?.kind() != close {
        if object {
            let key = take(tokens, position, TokenKind::Identifier)?;
            after_stop |= !keys.insert(spelling(text, key))
                || matches!(spelling(text, key), "constructor" | "prototype" | "__proto__");
            let retained = children.len();
            if take(tokens, position, TokenKind::Colon).is_none() {
                children.push(push(
                    nodes,
                    super::super::super::raw(key),
                    "Identifier",
                    vec![],
                    false,
                ));
            } else {
                children.push(expression(tokens, position, text, nodes, version, nesting + 1)?);
            }
            if after_stop {
                children.truncate(retained);
            }
        } else {
            children.push(expression(tokens, position, text, nodes, version, nesting + 1)?);
        }
        if take(tokens, position, TokenKind::Comma).is_none() {
            break;
        }
    }
    take(tokens, position, close)?;
    stop |= children.len() > if object { 1024 } else { 4096 };
    Some((children, stop, after_stop))
}

pub(super) fn parse(
    tokens: &[Token],
    position: &mut usize,
    text: &str,
    nodes: &mut Vec<Node>,
    version: u32,
    nesting: u32,
    callee: Token,
) -> Option<u32> {
    let open =
        (*position..tokens.len()).find(|index| tokens[*index].kind() == TokenKind::OpenParen)?;
    parse_inner(tokens, position, text, nodes, version, nesting, callee).or_else(|| {
        let mut closers = Vec::new();
        for (index, token) in tokens.iter().enumerate().skip(open) {
            match token.kind() {
                TokenKind::OpenParen => closers.push(TokenKind::CloseParen),
                TokenKind::OpenBrace => closers.push(TokenKind::CloseBrace),
                TokenKind::OpenBracket => closers.push(TokenKind::CloseBracket),
                TokenKind::CloseParen | TokenKind::CloseBrace | TokenKind::CloseBracket => {
                    if closers.pop() != Some(token.kind()) {
                        return None;
                    }
                    if closers.is_empty() {
                        *position = index + 1;
                        let span = UntrustedSpan {
                            file: callee.span().file().index(),
                            start: callee.span().start(),
                            end: token.span().end(),
                        };
                        return Some(push(nodes, span, "CallExpression", vec![], true));
                    }
                }
                _ => {}
            }
        }
        None
    })
}

fn parse_inner(
    tokens: &[Token],
    position: &mut usize,
    text: &str,
    nodes: &mut Vec<Node>,
    version: u32,
    nesting: u32,
    callee: Token,
) -> Option<u32> {
    let name = spelling(text, callee);
    let (types, type_stop) = type_arguments(tokens, position, text)?;
    take(tokens, position, TokenKind::OpenParen)?;
    let deferred = version == 4 && matches!(name, "Vec" | "FixedArray");
    let mut stop = !deferred && (type_stop || types > 0);
    let mut arguments = 0;
    let mut children = Vec::new();
    let mut array = false;
    let mut object = false;
    let mut after_stop = false;
    let mut before_operands = 0;
    while tokens.get(*position)?.kind() != TokenKind::CloseParen {
        let token = tokens[*position];
        let (mut values, invalid, deferred_invalid) = if version == 4
            && name == "match"
            && arguments == 1
        {
            let (values, invalid) =
                super::match_arms::parse(tokens, position, text, nodes, nesting)?;
            after_stop |= invalid;
            (values, false, false)
        } else if matches!(token.kind(), TokenKind::OpenBrace | TokenKind::OpenBracket) {
            array |= token.kind() == TokenKind::OpenBracket;
            object |= token.kind() == TokenKind::OpenBrace;
            if version == 4 && token.kind() == TokenKind::OpenBrace {
                before_operands += super::match_arms::object_end(tokens, *position)?.1;
            }
            stop |= version == 3;
            collection(
                tokens,
                position,
                text,
                nodes,
                version,
                nesting,
                token.kind() == TokenKind::OpenBrace,
            )?
        } else {
            (vec![expression(tokens, position, text, nodes, version, nesting + 1)?], false, false)
        };
        children.append(&mut values);
        stop |= invalid;
        after_stop |= deferred_invalid;
        arguments += 1;
        if take(tokens, position, TokenKind::Comma).is_none() {
            break;
        }
    }
    let close = take(tokens, position, TokenKind::CloseParen)?;
    stop |= arguments > 256;
    stop |= object && before_operands > 1024;
    if version == 4 {
        stop |= array && !matches!(name, "Vec" | "FixedArray")
            || object
                && (arguments != 1
                    || matches!(name, "clone" | "borrow" | "borrowMut" | "shared" | "downgrade"));
        stop |= match name {
            "clone" | "borrow" | "borrowMut" | "shared" | "downgrade" => arguments != 1,
            "push" | "match" => arguments != 2,
            "Vec" => arguments != 1 || types != 1 || !array,
            "FixedArray" => arguments != 1 || types != 2 || !array,
            "upgradeWeak" => true,
            _ => false,
        };
    }
    if stop {
        children.clear();
    }
    let span = UntrustedSpan {
        file: callee.span().file().index(),
        start: callee.span().start(),
        end: close.span().end(),
    };
    let operands = if deferred { children.len() } else { 0 };
    let id = push(nodes, span, "CallExpression", children, stop);
    nodes[id as usize].after_stop = after_stop || deferred && type_stop;
    nodes[id as usize].operands = operands;
    nodes[id as usize].before_operands = before_operands;
    Some(id)
}
