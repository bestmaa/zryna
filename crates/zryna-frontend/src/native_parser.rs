//! Internal native syntax candidate construction from the bound lexical stream.
//!
//! This first closed slice constructs protocol-v2 function/return/addition candidates. It does
//! not select a provider or grant syntax authority: callers must use the existing v2 verifier.

use std::fmt;

use zryna_diagnostics::Diagnostic;
use zryna_source::{SourceMap, UntrustedSpan};
use zryna_syntax::v2 as syntax;

use crate::native_lexer::{Keyword, LexedProject, Token, TokenKind};

mod expression;
mod recovery;

/// A deterministic rejection of source outside this native candidate slice.
#[derive(Clone, Debug)]
pub struct ParseError(Diagnostic);

impl ParseError {
    /// Returns the source-bound diagnostic.
    #[must_use]
    pub const fn diagnostic(&self) -> &Diagnostic {
        &self.0
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

impl std::error::Error for ParseError {}

/// Constructs one untrusted protocol-v2 candidate from the exact native lexical output.
///
/// This deliberately accepts only the closed `export function` / `return` subset represented by
/// protocol v2. Missing annotations and semicolon insertion use the frozen bootstrap spans;
/// unsupported syntax is rejected until its recovery has independent differential coverage.
///
/// # Errors
///
/// Rejects a mismatched source map, lexical diagnostics, unsupported syntax, or first-extra
/// protocol-v2 inventory before exposing any partial candidate.
pub fn parse_v2_candidate(
    sources: &SourceMap,
    lexed: &LexedProject,
) -> Result<syntax::RawProjectSyntaxSnapshot, ParseError> {
    parse_v2_internal(sources, lexed, false)
}

/// Constructs a bounded, untrusted v2 candidate that retains parser errors as diagnostics.
///
/// Each unsupported top-level declaration or function is discarded atomically. Recovery resumes
/// at the next top-level `export` token after balanced braces, never inside the rejected body.
/// The existing verifier can check the returned DTO, but its error diagnostics must stop semantic
/// input construction. Lexical errors and resource overflow still return [`ParseError`].
///
/// # Errors
///
/// Returns an error for foreign source maps, lexical errors, or first-extra budgets.
pub fn parse_v2_recovering_candidate(
    sources: &SourceMap,
    lexed: &LexedProject,
) -> Result<syntax::RawProjectSyntaxSnapshot, ParseError> {
    parse_v2_internal(sources, lexed, true)
}

fn parse_v2_internal(
    sources: &SourceMap,
    lexed: &LexedProject,
    recovering: bool,
) -> Result<syntax::RawProjectSyntaxSnapshot, ParseError> {
    if !lexed.is_bound_to(sources) || lexed.files().len() != sources.len() {
        return Err(failure("ZRYNA-F2002", "native tokens do not belong to this source map"));
    }
    if let Some(diagnostic) = lexed.diagnostics().first() {
        return Err(ParseError(diagnostic.clone()));
    }
    let mut files = Vec::with_capacity(lexed.files().len());
    let mut total_functions = 0_usize;
    let mut total_parameters = 0_usize;
    let mut total_statements = 0_usize;
    let mut total_expressions = 0_usize;
    let mut diagnostics = Vec::new();
    for file in lexed.files() {
        let source = sources
            .source(file.id())
            .ok_or_else(|| failure("ZRYNA-F2002", "native source file is unavailable"))?;
        if source.path() != file.path() {
            return Err(failure("ZRYNA-F2002", "native source path differs from the source map"));
        }
        let mut parser = FileParser {
            text: source.text(),
            tokens: file.tokens().collect(),
            position: 0,
            file: file.id().index(),
        };
        let mut functions = Vec::new();
        while parser.current().is_some() {
            if functions.len() >= syntax::MAX_FUNCTIONS_PER_FILE
                || total_functions >= syntax::MAX_FUNCTIONS_PER_PROJECT
            {
                return Err(parser
                    .error_here("ZRYNA-F2003", "function inventory exceeds protocol-v2 limit"));
            }
            let checkpoint = parser.position;
            let function = match parser.function() {
                Ok(function) => function,
                Err(error) if recovering && error.diagnostic().code() == "ZRYNA-F2002" => {
                    if diagnostics.len() >= syntax::MAX_PROVIDER_DIAGNOSTICS {
                        return Err(resource("parser diagnostics exceed protocol-v2 limit"));
                    }
                    diagnostics.push(recovery::raw_diagnostic(&error));
                    recovery::skip_to_next_function(&mut parser, checkpoint);
                    continue;
                }
                Err(error) => return Err(error),
            };
            total_functions += 1;
            total_parameters += function.parameters.len();
            total_statements += function.body.statements.len();
            total_expressions += function.body.expressions.len();
            if total_parameters > syntax::MAX_PARAMETERS_PER_PROJECT
                || total_statements > syntax::MAX_STATEMENTS_PER_PROJECT
                || total_expressions > syntax::MAX_EXPRESSIONS_PER_PROJECT
            {
                return Err(resource("project syntax inventory exceeds protocol-v2 limit"));
            }
            functions.push(function);
        }
        files.push(syntax::RawSourceUnit {
            id: file.id().index(),
            path: file.path().as_str().to_owned(),
            functions,
        });
    }
    Ok(syntax::RawProjectSyntaxSnapshot {
        schema_version: syntax::PROTOCOL_VERSION,
        files,
        diagnostics,
    })
}

struct FileParser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    position: usize,
    file: u32,
}

impl FileParser<'_> {
    fn error_here(&self, code: &'static str, message: &'static str) -> ParseError {
        self.current()
            .or_else(|| self.tokens.last().copied())
            .map_or_else(|| failure(code, message), |token| error_at(token, code, message))
    }

    fn current(&self) -> Option<Token> {
        self.tokens.get(self.position).copied()
    }

    fn take(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        let token = self
            .current()
            .ok_or_else(|| self.error_here("ZRYNA-F2002", "unexpected end of source"))?;
        if token.kind() != kind {
            return Err(self.error_here("ZRYNA-F2002", "unexpected token in protocol-v2 source"));
        }
        self.position += 1;
        Ok(token)
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
        let token = self.take(TokenKind::Identifier)?;
        if matches!(
            self.spelling(token),
            "this" | "null" | "super" | "new" | "typeof" | "void" | "delete" | "class"
        ) {
            return Err(error_at(token, "ZRYNA-F2002", "unsupported identifier spelling"));
        }
        Ok(syntax::RawIdentifierSyntax { text: self.spelling(token).to_owned(), span: raw(token) })
    }

    fn spelling(&self, token: Token) -> &str {
        &self.text[token.span().start() as usize..token.span().end() as usize]
    }

    fn annotation(&mut self, insertion: u32) -> Result<syntax::RawTypeSyntax, ParseError> {
        if self.maybe(TokenKind::Colon).is_none() {
            return Ok(syntax::RawTypeSyntax {
                span: UntrustedSpan { file: self.file, start: insertion, end: insertion },
                kind: syntax::RawTypeSyntaxKind::Missing,
            });
        }
        let token = self
            .current()
            .ok_or_else(|| self.error_here("ZRYNA-F2002", "missing type annotation"))?;
        match token.kind() {
            TokenKind::Identifier => {}
            _ => return Err(self.error_here("ZRYNA-F2002", "unsupported type annotation")),
        }
        if matches!(
            self.spelling(token),
            "unknown"
                | "never"
                | "number"
                | "string"
                | "boolean"
                | "symbol"
                | "bigint"
                | "undefined"
                | "object"
                | "infer"
                | "keyof"
                | "readonly"
                | "unique"
        ) {
            return Err(error_at(token, "ZRYNA-F2002", "unsupported type annotation"));
        }
        self.position += 1;
        Ok(syntax::RawTypeSyntax {
            span: raw(token),
            kind: syntax::RawTypeSyntaxKind::Named { name: self.spelling(token).to_owned() },
        })
    }

    fn function(&mut self) -> Result<syntax::RawFunctionSyntax, ParseError> {
        let export = self.take(TokenKind::Keyword(Keyword::Export))?;
        let keyword = self.take(TokenKind::Keyword(Keyword::Function))?;
        let name = self.identifier()?;
        let (parameters, parameter_end) = self.parameters()?;
        let result_type = self.annotation(parameter_end)?;
        let open = self.take(TokenKind::OpenBrace)?;
        let mut statements = Vec::new();
        let mut expressions = Vec::new();
        while self.current().is_some_and(|token| token.kind() != TokenKind::CloseBrace) {
            if statements.len() >= syntax::MAX_STATEMENTS_PER_FUNCTION {
                return Err(self.error_here(
                    "ZRYNA-F2003",
                    "function statement inventory exceeds protocol-v2 limit",
                ));
            }
            let keyword = self.take(TokenKind::Keyword(Keyword::Return))?;
            if self.current().is_some_and(|next| {
                has_line_break(self.text, keyword.span().end(), next.span().start())
            }) {
                return Err(error_at(
                    keyword,
                    "ZRYNA-F2002",
                    "return value must start on the same line",
                ));
            }
            let expression_start = self.position;
            let value = expression::addition(self, &mut expressions)?;
            if self.position == expression_start {
                return Err(self.error_here("ZRYNA-F2002", "return value is missing"));
            }
            let root = &expressions[value as usize];
            let statement_end = if let Some(semicolon) = self.maybe(TokenKind::Semicolon) {
                semicolon.span().end()
            } else if self.current().is_some_and(|next| {
                next.kind() == TokenKind::CloseBrace
                    || (next.kind() == TokenKind::Keyword(Keyword::Return)
                        && has_line_break(self.text, root.span.end, next.span().start()))
            }) {
                root.span.end
            } else {
                return Err(self.error_here("ZRYNA-F2002", "missing return statement terminator"));
            };
            statements.push(syntax::RawStatementSyntax {
                span: UntrustedSpan {
                    file: self.file,
                    start: keyword.span().start(),
                    end: statement_end,
                },
                kind: syntax::RawStatementKind::Return { keyword_span: raw(keyword), value },
            });
        }
        let close = self.take(TokenKind::CloseBrace)?;
        Ok(syntax::RawFunctionSyntax {
            span: UntrustedSpan {
                file: self.file,
                start: export.span().start(),
                end: close.span().end(),
            },
            export_span: raw(export),
            function_span: raw(keyword),
            name,
            parameters,
            result_type,
            body: syntax::RawFunctionBodySyntax {
                span: UntrustedSpan {
                    file: self.file,
                    start: open.span().start(),
                    end: close.span().end(),
                },
                statements,
                expressions,
            },
        })
    }

    fn parameters(&mut self) -> Result<(Vec<syntax::RawParameterSyntax>, u32), ParseError> {
        let open_paren = self.take(TokenKind::OpenParen)?;
        let mut parameter_end = open_paren.span().end();
        let mut parameters = Vec::new();
        if self.current().is_some_and(|token| token.kind() != TokenKind::CloseParen) {
            loop {
                if parameters.len() >= syntax::MAX_PARAMETERS_PER_FUNCTION {
                    return Err(self.error_here(
                        "ZRYNA-F2003",
                        "function parameter inventory exceeds protocol-v2 limit",
                    ));
                }
                let name = self.identifier()?;
                let type_syntax = self.annotation(name.span.end)?;
                parameter_end = type_syntax.span.end;
                parameters.push(syntax::RawParameterSyntax {
                    span: UntrustedSpan {
                        file: self.file,
                        start: name.span.start,
                        end: type_syntax.span.end,
                    },
                    name,
                    type_syntax,
                });
                let Some(comma) = self.maybe(TokenKind::Comma) else {
                    break;
                };
                parameter_end = comma.span().end();
                if self.current().is_some_and(|token| token.kind() == TokenKind::CloseParen) {
                    break;
                }
            }
        }
        self.take(TokenKind::CloseParen)?;
        Ok((parameters, parameter_end))
    }
}

fn raw(token: Token) -> UntrustedSpan {
    let span = token.span();
    UntrustedSpan { file: span.file().index(), start: span.start(), end: span.end() }
}

fn has_line_break(text: &str, start: u32, end: u32) -> bool {
    text[start as usize..end as usize]
        .chars()
        .any(|character| matches!(character, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
}

fn failure(code: &'static str, message: &'static str) -> ParseError {
    ParseError(Diagnostic::error(code, None, message, "use the supported native syntax subset"))
}

fn error_at(token: Token, code: &'static str, message: &'static str) -> ParseError {
    ParseError(Diagnostic::error_at(
        code,
        token.span(),
        message,
        "use the supported native syntax subset",
    ))
}

fn resource(message: &'static str) -> ParseError {
    failure("ZRYNA-F2003", message)
}
