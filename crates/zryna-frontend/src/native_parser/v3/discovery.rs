//! Import-only discovery over original token streams; never executable syntax authority.

use zryna_source::{SourceMap, UntrustedSpan};
use zryna_syntax::v3 as syntax;

use super::{FileParser, ParseError, failure, resource};
use crate::native_lexer::{Keyword, LexedProject, TokenKind};

/// Untrusted discovery records, deliberately distinct from executable syntax snapshot DTOs.
pub struct RawModuleImports {
    /// Original snapshot-local source identifier.
    pub id: u32,
    /// Original canonical source path.
    pub path: String,
    /// Source-ordered named-import candidates with original byte offsets.
    pub imports: Vec<syntax::RawImportSyntax>,
}

/// Extracts untrusted top-level import candidates without parsing function or data bodies.
///
/// Original sources and offsets remain unchanged. The complete native parser and syntax verifier
/// must subsequently authenticate the exact file set and all discovered imports. This result
/// cannot enter semantics or grant filesystem authority.
///
/// # Errors
/// Rejects foreign lexical authority, lexical errors, malformed named imports and import budgets.
pub fn discover_import_candidates(
    sources: &SourceMap,
    lexed: &LexedProject,
) -> Result<Vec<RawModuleImports>, ParseError> {
    if !lexed.is_bound_to(sources) || lexed.files().len() != sources.len() {
        return Err(failure("ZRYNA-F2002", "native tokens do not belong to this source map"));
    }
    if let Some(diagnostic) = lexed.diagnostics().first() {
        return Err(ParseError { diagnostic: diagnostic.clone() });
    }
    let mut result = Vec::with_capacity(sources.len());
    let mut total_imports = 0;
    let mut total_bindings = 0;
    for file in lexed.files() {
        let source = sources
            .source(file.id())
            .ok_or_else(|| failure("ZRYNA-F2002", "native source is unavailable"))?;
        let end = u32::try_from(source.text().len())
            .map_err(|_| resource("source length exceeds the discovery limit"))?;
        let eof = sources
            .verify_span(UntrustedSpan { file: file.id().index(), start: end, end })
            .map_err(|_| failure("ZRYNA-F2002", "native EOF is unavailable"))?;
        let mut parser = FileParser {
            sources,
            text: source.text(),
            tokens: file.tokens().collect(),
            position: 0,
            file: file.id().index(),
            eof,
        };
        let mut delimiters = Vec::new();
        let mut imports = Vec::new();
        while let Some(token) = parser.current() {
            if delimiters.is_empty() && token.kind() == TokenKind::Keyword(Keyword::Import) {
                if imports.len() >= syntax::MAX_IMPORTS_PER_MODULE
                    || total_imports >= syntax::MAX_IMPORTS_PER_PROJECT
                {
                    return Err(resource("source discovery exceeds the import limit"));
                }
                let import = parser.import(total_bindings)?;
                total_imports += 1;
                total_bindings += import.bindings.len();
                imports.push(import);
                continue;
            }
            match token.kind() {
                TokenKind::OpenBrace => delimiters.push(TokenKind::CloseBrace),
                TokenKind::OpenParen => delimiters.push(TokenKind::CloseParen),
                TokenKind::OpenBracket => delimiters.push(TokenKind::CloseBracket),
                TokenKind::CloseBrace | TokenKind::CloseParen | TokenKind::CloseBracket
                    if delimiters.pop() != Some(token.kind()) =>
                {
                    return Err(parser.error_here("unbalanced source during import discovery"));
                }
                _ => {}
            }
            parser.position += 1;
        }
        if !delimiters.is_empty() {
            return Err(parser.error_here("unbalanced source during import discovery"));
        }
        result.push(RawModuleImports {
            id: file.id().index(),
            path: file.path().as_str().to_owned(),
            imports,
        });
    }
    Ok(result)
}
