//! Internal source-bound protocol-v5 candidate construction.
//!
//! This isolated sibling derives raw arenas from the immutable native lexical project. It does
//! not select a provider or resolve imports/types. Callers must cross the existing v5 verifier;
//! only that barrier creates an immutable source-bound syntax seal. Contextual-role rejection
//! may occur here before the bootstrap raw DTO reaches that barrier. Full excluded-source
//! diagnostic equivalence and the existing diagnostic-only context gaps remain unclaimed.

use std::fmt;

use zryna_diagnostics::Diagnostic;
use zryna_source::{SourceMap, UntrustedSpan};
mod syntax;

use crate::native_lexer::{Keyword, LexedProject, Token, TokenKind};

mod data;
mod functions;
mod generics;
mod imports;
mod names;
mod types;

/// One deterministic rejection of the native protocol-v5 candidate.
#[derive(Clone, Debug)]
pub struct ParseError {
    diagnostic: Diagnostic,
}

impl ParseError {
    /// Returns the source-bound or resource diagnostic.
    #[must_use]
    pub const fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.diagnostic, formatter)
    }
}

impl std::error::Error for ParseError {}

fn unsupported(token: Option<Token>, message: &'static str) -> ParseError {
    let diagnostic = token.map_or_else(
        || Diagnostic::error("ZRYNA-F2002", None, message, "use supported protocol-v5 syntax"),
        |token| {
            Diagnostic::error_at(
                "ZRYNA-F2002",
                token.span(),
                message,
                "use supported protocol-v5 syntax",
            )
        },
    );
    ParseError { diagnostic }
}

fn resource(message: &'static str) -> ParseError {
    ParseError {
        diagnostic: Diagnostic::error(
            "ZRYNA-F1002",
            None,
            message,
            "keep the candidate within protocol-v5 resource limits",
        ),
    }
}

fn raw(token: Token) -> UntrustedSpan {
    UntrustedSpan {
        file: token.span().file().index(),
        start: token.span().start(),
        end: token.span().end(),
    }
}

fn valid_name(name: &str) -> bool {
    name.len() <= 128
        && !matches!(name, "__proto__" | "prototype" | "constructor")
        && name.as_bytes().first().is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
        && name.as_bytes()[1..].iter().all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn valid_specifier(value: &str) -> bool {
    let body = value.strip_prefix("./").unwrap_or_else(|| value.trim_start_matches("../"));
    !body.is_empty()
        && !value.is_empty()
        && value.len() <= zryna_syntax::v3::MAX_MODULE_SPECIFIER_BYTES
        && value.is_ascii()
        && (value.starts_with("./") || value.starts_with("../"))
        && value.ends_with(".zry")
        && !value.contains(['\\', '?', '#', '\0'])
        && !value.contains("://")
        && !value.split('/').any(str::is_empty)
}

struct FileParser<'a> {
    external_module: bool,
    in_function: bool,
    text: &'a str,
    tokens: Vec<Token>,
    position: usize,
    pending_type_equals: bool,
    file: u32,
    types: Vec<syntax::RawTypeSyntax>,
    previous_types: usize,
    previous_members: usize,
    previous_bindings: usize,
    previous_parameters: usize,
    previous_blocks: usize,
    previous_statements: usize,
    previous_expressions: usize,
    previous_locals: usize,
    aggregate_operands: usize,
    match_arms: usize,
}

#[derive(Default)]
struct Totals {
    declarations: usize,
    types: usize,
    members: usize,
    imports: usize,
    bindings: usize,
    functions: usize,
    parameters: usize,
    blocks: usize,
    statements: usize,
    expressions: usize,
    locals: usize,
    aggregate_operands: usize,
    match_arms: usize,
}

impl FileParser<'_> {
    fn current(&self) -> Option<Token> {
        self.tokens.get(self.position).copied()
    }

    fn next(&self) -> Option<Token> {
        self.tokens.get(self.position + 1).copied()
    }

    fn spelling(&self, token: Token) -> &str {
        &self.text[token.span().start() as usize..token.span().end() as usize]
    }

    fn take(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        let token = self
            .current()
            .ok_or_else(|| unsupported(self.tokens.last().copied(), "incomplete syntax"))?;
        if token.kind() != kind {
            return Err(unsupported(Some(token), "unsupported protocol-v5 syntax"));
        }
        self.position += 1;
        Ok(token)
    }

    fn initializer_equals(&mut self) -> Result<UntrustedSpan, ParseError> {
        if self.pending_type_equals {
            let token = self.take(TokenKind::GreaterEqual)?;
            self.pending_type_equals = false;
            return Ok(UntrustedSpan { start: token.span().start() + 1, ..raw(token) });
        }
        self.take(TokenKind::Equals).map(raw)
    }

    fn maybe(&mut self, kind: TokenKind) -> Option<Token> {
        if self.current().is_some_and(|token| token.kind() == kind) {
            let token = self.current();
            self.position += 1;
            token
        } else {
            None
        }
    }

    fn identifier(&mut self) -> Result<syntax::RawIdentifierSyntax, ParseError> {
        let token = self
            .current()
            .filter(|token| matches!(token.kind(), TokenKind::Identifier | TokenKind::Keyword(_)))
            .ok_or_else(|| unsupported(self.current(), "expected identifier"))?;
        self.position += 1;
        let text = self.spelling(token);
        if !valid_name(text) {
            return Err(unsupported(Some(token), "invalid protocol-v5 identifier"));
        }
        Ok(syntax::RawIdentifierSyntax { text: text.to_owned(), span: raw(token) })
    }
}

/// Constructs an untrusted protocol-v5 candidate for the admitted native source subset.
///
/// The existing v4 verifier must authenticate its exact source map and all DTO edges.
///
/// # Errors
///
/// Rejects foreign source authority, lexical errors, unsupported syntax, and first-extra budgets.
pub fn parse_v5_candidate(
    sources: &SourceMap,
    lexed: &LexedProject,
) -> Result<syntax::RawProjectSyntaxSnapshot, ParseError> {
    parse_candidate(sources, lexed)
}

fn parse_candidate(
    sources: &SourceMap,
    lexed: &LexedProject,
) -> Result<syntax::RawProjectSyntaxSnapshot, ParseError> {
    if !lexed.is_bound_to(sources) || lexed.files().len() != sources.len() {
        return Err(unsupported(None, "native tokens do not belong to this source map"));
    }
    if let Some(diagnostic) = lexed.diagnostics().first() {
        return Err(ParseError { diagnostic: diagnostic.clone() });
    }
    let mut files = Vec::with_capacity(lexed.files().len());
    let mut totals = Totals::default();
    for file in lexed.files() {
        let source = sources
            .source(file.id())
            .ok_or_else(|| unsupported(None, "native source unavailable"))?;
        if source.path() != file.path() {
            return Err(unsupported(None, "native source path differs from the source map"));
        }
        let mut parser = FileParser {
            external_module: names::external_module(&file.tokens().collect::<Vec<_>>()),
            in_function: false,
            text: source.text(),
            tokens: file.tokens().collect(),
            position: 0,
            pending_type_equals: false,
            file: file.id().index(),
            types: Vec::new(),
            previous_types: totals.types,
            previous_members: totals.members,
            previous_bindings: totals.bindings,
            previous_parameters: totals.parameters,
            previous_blocks: totals.blocks,
            previous_statements: totals.statements,
            previous_expressions: totals.expressions,
            previous_locals: totals.locals,
            aggregate_operands: totals.aggregate_operands,
            match_arms: totals.match_arms,
        };
        enforce_source_nesting(&parser.tokens)?;
        files.push(parse_module(&mut parser, file.path().as_str(), &mut totals)?);
    }
    Ok(syntax::RawProjectSyntaxSnapshot {
        schema_version: syntax::PROTOCOL_VERSION,
        files,
        diagnostics: Vec::new(),
    })
}

fn parse_module(
    parser: &mut FileParser<'_>,
    path: &str,
    totals: &mut Totals,
) -> Result<syntax::RawSourceUnit, ParseError> {
    let mut imports = Vec::new();
    let mut declarations = Vec::new();
    let mut functions = Vec::new();
    while let Some(token) = parser.current() {
        if token.kind() == TokenKind::Keyword(Keyword::Import)
            && declarations.is_empty()
            && functions.is_empty()
        {
            if imports.len() >= syntax::MAX_IMPORTS_PER_MODULE
                || totals.imports + imports.len() >= syntax::MAX_IMPORTS_PER_PROJECT
            {
                return Err(resource(if imports.len() >= syntax::MAX_IMPORTS_PER_MODULE {
                    "module exceeds the import-declaration limit"
                } else {
                    "project exceeds the import-declaration limit"
                }));
            }
            let import = parser.import()?;
            parser.previous_bindings += import.bindings.len();
            imports.push(import);
            continue;
        }
        if !matches!(
            token.kind(),
            TokenKind::Keyword(Keyword::Interface | Keyword::Export | Keyword::Function)
        ) {
            return Err(unsupported(Some(token), "unsupported protocol-v5 top-level syntax"));
        }
        if token.kind() == TokenKind::Keyword(Keyword::Function)
            || (token.kind() == TokenKind::Keyword(Keyword::Export)
                && parser
                    .next()
                    .is_some_and(|next| next.kind() == TokenKind::Keyword(Keyword::Function)))
        {
            if functions.len() >= syntax::MAX_FUNCTIONS_PER_MODULE
                || totals.functions + functions.len() >= syntax::MAX_FUNCTIONS_PER_PROJECT
            {
                return Err(resource(if functions.len() >= syntax::MAX_FUNCTIONS_PER_MODULE {
                    "module exceeds the function limit"
                } else {
                    "project exceeds the function limit"
                }));
            }
            let function = parser.function()?;
            parser.previous_parameters += function.parameters.len();
            parser.previous_blocks += function.body.blocks.len();
            parser.previous_statements += function.body.statements.len();
            parser.previous_expressions += function.body.expressions.len();
            let locals = function
                .body
                .statements
                .iter()
                .filter(|statement| {
                    matches!(statement.kind, syntax::RawStatementKind::LocalDeclaration { .. })
                })
                .count();
            parser.previous_locals += locals;
            totals.parameters += function.parameters.len();
            totals.blocks += function.body.blocks.len();
            totals.statements += function.body.statements.len();
            totals.expressions += function.body.expressions.len();
            totals.locals += locals;
            functions.push(function);
            continue;
        }
        if declarations.len() >= syntax::MAX_DATA_DECLARATIONS_PER_MODULE {
            return Err(resource("module exceeds the nominal-declaration limit"));
        }
        let declaration = parser.data_declaration(totals.declarations + declarations.len())?;
        totals.members += match &declaration.kind {
            syntax::RawDataDeclarationKind::Struct { fields, .. } => fields.len(),
            syntax::RawDataDeclarationKind::Enum { variants, .. } => variants.len(),
        };
        parser.previous_members = totals.members;
        declarations.push(declaration);
    }
    totals.declarations += declarations.len();
    totals.functions += functions.len();
    totals.types += parser.types.len();
    totals.imports += imports.len();
    totals.bindings = parser.previous_bindings;
    totals.aggregate_operands = parser.aggregate_operands;
    totals.match_arms = parser.match_arms;
    Ok(syntax::RawSourceUnit {
        id: parser.file,
        path: path.to_owned(),
        imports,
        type_syntax: std::mem::take(&mut parser.types),
        data_declarations: declarations,
        functions,
    })
}

fn enforce_source_nesting(tokens: &[Token]) -> Result<(), ParseError> {
    let mut depth = 0_u32;
    for token in tokens {
        match token.kind() {
            TokenKind::OpenBrace | TokenKind::OpenBracket | TokenKind::OpenParen => {
                depth += 1;
                if depth > syntax::MAX_NESTING_DEPTH {
                    return Err(resource("source exceeds the nesting limit"));
                }
            }
            TokenKind::CloseBrace | TokenKind::CloseBracket | TokenKind::CloseParen => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    Ok(())
}
