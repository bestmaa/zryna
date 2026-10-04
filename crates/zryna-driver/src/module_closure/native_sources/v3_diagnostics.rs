//! M2 rejection presentation over the same bound tokens and retained source graph.

use std::collections::{BTreeMap, BTreeSet};

use zryna_diagnostics::Diagnostic;
use zryna_frontend::{
    native_lexer::{Keyword, LexedProject, TokenKind},
    native_parser::v3::ParseError,
};
use zryna_source::SourceMap;

use super::ModuleClosureError;
use crate::module_closure::{ModuleEdge, ModuleRecord, checked_add, invariant_rejection};

pub(super) fn discovery_error(
    sources: &SourceMap,
    lexed: &LexedProject,
    error: &ParseError,
) -> ModuleClosureError {
    let original = error.diagnostic();
    // Match the already-rejected, top-level bare import using exact bound tokens. A parser
    // message, fixture path or blanket F2002 translation cannot identify this RPC-level case.
    if lexed.is_bound_to(sources)
        && lexed.diagnostics().is_empty()
        && original.code() == "ZRYNA-F2002"
    {
        for file in lexed.files() {
            let mut delimiters = Vec::new();
            let mut import = false;
            for token in file.tokens() {
                if import
                    && token.kind() == TokenKind::StringLiteral
                    && original.primary_span() == Some(token.span())
                {
                    return ModuleClosureError::Rejected(vec![Diagnostic::error(
                        "ZRYNA-F1103",
                        None,
                        "ZRYNA-F1103: frontend worker rejected a protocol request",
                        "verify the pinned Node.js runtime and TypeScript frontend, then retry",
                    )]);
                }
                import =
                    delimiters.is_empty() && token.kind() == TokenKind::Keyword(Keyword::Import);
                match token.kind() {
                    TokenKind::OpenBrace => delimiters.push(TokenKind::CloseBrace),
                    TokenKind::OpenParen => delimiters.push(TokenKind::CloseParen),
                    TokenKind::OpenBracket => delimiters.push(TokenKind::CloseBracket),
                    TokenKind::CloseBrace | TokenKind::CloseParen | TokenKind::CloseBracket
                        if delimiters.pop() != Some(token.kind()) =>
                    {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    ModuleClosureError::Rejected(vec![original.clone()])
}

pub(super) fn reject_cycles(
    modules: &[ModuleRecord],
    edges: &[ModuleEdge],
) -> Result<(), ModuleClosureError> {
    // Match canonical M2's deterministic Kahn traversal, including its first remaining path.
    // Inputs are the source-derived records authenticated by graph::authenticate; no source
    // content, AST, caller-provided diagnostic or ambient filesystem lookup participates.
    let mut outgoing = modules
        .iter()
        .map(|module| (module.path.clone(), BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    let mut indegree =
        modules.iter().map(|module| (module.path.clone(), 0_usize)).collect::<BTreeMap<_, _>>();
    for edge in edges {
        if outgoing
            .get_mut(&edge.importer)
            .ok_or_else(invariant_rejection)?
            .insert(edge.target.clone())
        {
            let count = indegree.get_mut(&edge.target).ok_or_else(invariant_rejection)?;
            *count = checked_add(*count, 1)?;
        }
    }
    let mut ready = indegree
        .iter()
        .filter_map(|(path, count)| (*count == 0).then_some(path.clone()))
        .collect::<BTreeSet<_>>();
    let mut visited = 0;
    while let Some(path) = ready.pop_first() {
        visited = checked_add(visited, 1)?;
        for target in outgoing.get(&path).ok_or_else(invariant_rejection)? {
            let count = indegree.get_mut(target).ok_or_else(invariant_rejection)?;
            *count = count.checked_sub(1).ok_or_else(invariant_rejection)?;
            if *count == 0 {
                ready.insert(target.clone());
            }
        }
    }
    if visited != modules.len() {
        let path = indegree
            .iter()
            .find_map(|(path, count)| (*count > 0).then_some(path))
            .ok_or_else(invariant_rejection)?;
        return Err(ModuleClosureError::Rejected(vec![Diagnostic::error(
            "ZRYNA-D3007",
            None,
            format!("{path}: module import graph contains a cycle"),
            "remove self imports and cyclic dependency paths",
        )]));
    }
    Ok(())
}
