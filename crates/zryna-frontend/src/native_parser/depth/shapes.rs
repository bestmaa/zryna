//! Lexer-bounded expression shapes used before candidate inventory allocation.

use crate::native_lexer::{Keyword, Token, TokenKind};
use zryna_diagnostics::Diagnostic;
use zryna_source::{SourceMap, UntrustedSpan};

mod calls;
mod match_arms;

struct Node {
    span: UntrustedSpan,
    kind: &'static str,
    children: Vec<u32>,
    depth: u32,
    stop: bool,
    after_stop: bool,
    operands: usize,
    before_operands: usize,
}

fn push(
    nodes: &mut Vec<Node>,
    span: UntrustedSpan,
    kind: &'static str,
    children: Vec<u32>,
    stop: bool,
) -> u32 {
    let depth = 1 + children.iter().map(|id| nodes[*id as usize].depth).max().unwrap_or(0);
    let id = u32::try_from(nodes.len()).expect("lexer-bounded shape nodes");
    nodes.push(Node {
        span,
        kind,
        children,
        depth,
        stop,
        after_stop: false,
        operands: 0,
        before_operands: 0,
    });
    id
}

fn spelling(text: &str, token: Token) -> &str {
    &text[token.span().start() as usize..token.span().end() as usize]
}

fn take(tokens: &[Token], position: &mut usize, kind: TokenKind) -> Option<Token> {
    let token = *tokens.get(*position)?;
    if token.kind() != kind {
        return None;
    }
    *position += 1;
    Some(token)
}

fn atom(
    tokens: &[Token],
    position: &mut usize,
    text: &str,
    nodes: &mut Vec<Node>,
    version: u32,
    nesting: u32,
) -> Option<u32> {
    if nesting > 128 {
        return None;
    }
    let mut minuses: Vec<Token> = Vec::new();
    while let Some(token) = tokens.get(*position).filter(|token| token.kind() == TokenKind::Minus) {
        if minuses.last().is_some_and(|last| last.span().end() == token.span().start()) {
            return None;
        }
        minuses.push(*token);
        *position += 1;
    }
    let token = *tokens.get(*position)?;
    let (kind, stop) = match token.kind() {
        TokenKind::Identifier => match spelling(text, token) {
            "null" => ("NullKeyword", true),
            "this" => ("ThisKeyword", true),
            "super" | "new" | "typeof" | "void" | "delete" | "class" => ("Identifier", true),
            value => (
                "Identifier",
                version == 4 && matches!(value, "constructor" | "prototype" | "__proto__"),
            ),
        },
        TokenKind::DecimalInteger => {
            let literal = spelling(text, token);
            ("FirstLiteralToken", literal.len() > 64 || literal != "0" && literal.starts_with('0'))
        }
        TokenKind::StringLiteral => ("StringLiteral", version == 3),
        TokenKind::Keyword(Keyword::True) => ("TrueKeyword", false),
        TokenKind::Keyword(Keyword::False) => ("FalseKeyword", false),
        _ => return None,
    };
    *position += 1;
    let mut span = super::super::raw(token);
    let mut kind = kind;
    if token.kind() == TokenKind::DecimalInteger
        && let Some(minus) = minuses.last()
    {
        let literal = spelling(text, token);
        if minus.span().end() == token.span().start()
            && literal != "0"
            && !literal.starts_with('0')
            && literal.len() < 64
        {
            span.start = minus.span().start();
            kind = "PrefixUnaryExpression";
            minuses.pop();
        }
    }
    let mut id = push(nodes, span, kind, vec![], stop);
    if token.kind() == TokenKind::Identifier && calls::starts(tokens, *position) {
        id = calls::parse(tokens, position, text, nodes, version, nesting, token)?;
        nodes[id as usize].stop |= stop;
    }
    id = postfix(tokens, position, text, nodes, version, nesting, id)?;
    while let Some(minus) = minuses.pop() {
        let operand = nodes[id as usize].span;
        id = push(
            nodes,
            UntrustedSpan { file: operand.file, start: minus.span().start(), end: operand.end },
            "PrefixUnaryExpression",
            vec![id],
            false,
        );
    }
    Some(id)
}

fn postfix(
    tokens: &[Token],
    position: &mut usize,
    text: &str,
    nodes: &mut Vec<Node>,
    version: u32,
    nesting: u32,
    mut base: u32,
) -> Option<u32> {
    loop {
        let span = nodes[base as usize].span;
        match tokens.get(*position).map(|token| token.kind()) {
            Some(TokenKind::Dot) => {
                *position += 1;
                let name = take(tokens, position, TokenKind::Identifier)?;
                if tokens.get(*position).is_some_and(|token| token.kind() == TokenKind::OpenParen) {
                    let direct =
                        nodes[base as usize].kind == "Identifier" && !nodes[base as usize].stop;
                    let callee = tokens[tokens
                        .binary_search_by_key(&span.start, |token| token.span().start())
                        .ok()?];
                    base = calls::parse(tokens, position, text, nodes, version, nesting, callee)?;
                    let node = &mut nodes[base as usize];
                    node.stop |= version == 3 || !direct || node.children.len() > 1;
                    node.stop |=
                        matches!(spelling(text, name), "constructor" | "prototype" | "__proto__");
                    node.operands = node.children.len();
                } else {
                    base = push(
                        nodes,
                        UntrustedSpan { end: name.span().end(), ..span },
                        "PropertyAccessExpression",
                        vec![base],
                        version == 3,
                    );
                    nodes[base as usize].after_stop =
                        matches!(spelling(text, name), "constructor" | "prototype" | "__proto__");
                }
            }
            Some(TokenKind::OpenBracket) => {
                *position += 1;
                let index = expression(tokens, position, text, nodes, version, nesting + 1)?;
                let close = take(tokens, position, TokenKind::CloseBracket)?;
                base = push(
                    nodes,
                    UntrustedSpan { end: close.span().end(), ..span },
                    "ElementAccessExpression",
                    vec![base, index],
                    version == 3,
                );
            }
            _ => return Some(base),
        }
    }
}

fn precedence(kind: TokenKind) -> Option<u8> {
    match kind {
        TokenKind::StrictEqual | TokenKind::StrictNotEqual => Some(1),
        TokenKind::LessThan
        | TokenKind::LessEqual
        | TokenKind::GreaterThan
        | TokenKind::GreaterEqual => Some(2),
        TokenKind::Plus | TokenKind::Minus => Some(3),
        TokenKind::Asterisk => Some(4),
        _ => None,
    }
}

fn reduce(nodes: &mut Vec<Node>, values: &mut Vec<u32>) {
    let rhs = values.pop().expect("shape right operand");
    let lhs = values.pop().expect("shape left operand");
    let span = UntrustedSpan { end: nodes[rhs as usize].span.end, ..nodes[lhs as usize].span };
    values.push(push(nodes, span, "BinaryExpression", vec![lhs, rhs], false));
}

fn expression(
    tokens: &[Token],
    position: &mut usize,
    text: &str,
    nodes: &mut Vec<Node>,
    version: u32,
    nesting: u32,
) -> Option<u32> {
    let mut values = vec![atom(tokens, position, text, nodes, version, nesting)?];
    let mut operators = Vec::new();
    while let Some(token) = tokens.get(*position) {
        let Some(next) = precedence(token.kind()) else { break };
        if matches!(token.kind(), TokenKind::Plus | TokenKind::Minus)
            && tokens.get(*position + 1).is_some_and(|other| {
                other.kind() == token.kind() && token.span().end() == other.span().start()
            })
        {
            return None;
        }
        *position += 1;
        while operators.last().is_some_and(|current| *current >= next) {
            operators.pop();
            reduce(nodes, &mut values);
        }
        operators.push(next);
        values.push(atom(tokens, position, text, nodes, version, nesting)?);
    }
    while operators.pop().is_some() {
        reduce(nodes, &mut values);
    }
    values.pop()
}

pub(in crate::native_parser) fn diagnostic(
    sources: &SourceMap,
    text: &str,
    tokens: &[Token],
    block_depth: u32,
    version: u32,
    budgets: super::ExpressionBudgets,
) -> Option<Diagnostic> {
    let mut position = 0;
    let mut nodes = Vec::new();
    let root = expression(tokens, &mut position, text, &mut nodes, version, 1)?;
    if tokens.get(position).is_some_and(|token| {
        !matches!(
            token.kind(),
            TokenKind::Semicolon | TokenKind::CloseBrace | TokenKind::CloseParen | TokenKind::Comma
        )
    }) {
        return None;
    }
    if nodes[root as usize].depth + block_depth <= 128 {
        return None;
    }
    overflow(sources, &nodes, root, block_depth + 1, version, budgets)
}

fn overflow(
    sources: &SourceMap,
    nodes: &[Node],
    root: u32,
    depth: u32,
    version: u32,
    mut budgets: super::ExpressionBudgets,
) -> Option<Diagnostic> {
    let mut pending = vec![(root, depth, false)];
    while let Some((id, depth, after)) = pending.pop() {
        let node = &nodes[id as usize];
        if after {
            budgets.aggregate += node.operands;
            if budgets.aggregate > 65_536
                || node.after_stop
                || budgets.function >= 16_384
                || budgets.project >= 262_144
            {
                return None;
            }
            budgets.function += 1;
            budgets.project += 1;
            continue;
        }
        if depth > 128 {
            return Some(Diagnostic::error_at(
                "ZRYNA-F2002",
                sources.verify_span(node.span).expect("source-bound shape"),
                format!("expression depth uses unsupported syntax '{}'", node.kind),
                format!("use only the documented protocol-v{version} bootstrap syntax"),
            ));
        }
        if node.stop {
            return None;
        }
        budgets.aggregate += node.before_operands;
        if budgets.aggregate > 65_536 {
            return None;
        }
        pending.push((id, depth, true));
        pending.extend(node.children.iter().rev().map(|child| (*child, depth + 1, false)));
    }
    None
}
