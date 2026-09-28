//! Internal protocol-v3 candidate for named imports and straight-line scalar functions.

use zryna_diagnostics::Diagnostic;
use zryna_source::{SourceMap, UntrustedSpan};
use zryna_syntax::v3 as syntax;

use crate::native_lexer::{Keyword, LexedProject, Token, TokenKind};

use super::{FileParser, ParseError, raw};

mod blocks;
mod expression;

/// Constructs an untrusted v3 candidate for an import prefix followed by straight-line functions.
///
/// Every nontrivia token in every file must belong to the supported grammar. This entry is
/// separate from the import-only candidate used by native source closure. The returned DTO
/// requires [`syntax::verify_snapshot`] before downstream use.
///
/// # Errors
///
/// Rejects foreign source authority, lexical errors, unsupported syntax, and first-extra
/// inventories without returning a partial candidate.
pub fn parse_v3_straight_line_candidate(
    sources: &SourceMap,
    lexed: &LexedProject,
) -> Result<syntax::RawProjectSyntaxSnapshot, ParseError> {
    if !lexed.is_bound_to(sources) || lexed.files().len() != sources.len() {
        return Err(failure("native tokens do not belong to this source map"));
    }
    if let Some(diagnostic) = lexed.diagnostics().first() {
        return Err(ParseError { diagnostic: diagnostic.clone() });
    }
    let mut files = Vec::with_capacity(lexed.files().len());
    let mut total_imports = 0_usize;
    let mut total_bindings = 0_usize;
    let mut total_functions = 0_usize;
    let mut total_parameters = 0_usize;
    let mut total_blocks = 0_usize;
    let mut total_statements = 0_usize;
    let mut total_locals = 0_usize;
    let mut total_expressions = 0_usize;
    for file in lexed.files() {
        let source = sources
            .source(file.id())
            .ok_or_else(|| failure("native source file is unavailable"))?;
        if source.path() != file.path() {
            return Err(failure("native source path differs from the source map"));
        }
        let mut parser = FileParser {
            text: source.text(),
            tokens: file.tokens().collect(),
            position: 0,
            file: file.id().index(),
        };
        let mut imports = Vec::new();
        let mut functions = Vec::new();
        while let Some(token) = parser.current() {
            match token.kind() {
                TokenKind::Keyword(Keyword::Import) if functions.is_empty() => {
                    if imports.len() >= syntax::MAX_IMPORTS_PER_MODULE
                        || total_imports >= syntax::MAX_IMPORTS_PER_PROJECT
                    {
                        return Err(resource("import inventory exceeds protocol-v3 limit"));
                    }
                    let import = parser.import(total_bindings)?;
                    total_imports += 1;
                    total_bindings += import.bindings.len();
                    imports.push(import);
                }
                TokenKind::Keyword(Keyword::Export | Keyword::Function) => {
                    if functions.len() >= syntax::MAX_FUNCTIONS_PER_MODULE
                        || total_functions >= syntax::MAX_FUNCTIONS_PER_PROJECT
                    {
                        return Err(resource("function inventory exceeds protocol-v3 limit"));
                    }
                    let function = parser.function(
                        total_parameters,
                        total_blocks,
                        total_statements,
                        total_locals,
                        total_expressions,
                    )?;
                    total_functions += 1;
                    total_parameters += function.parameters.len();
                    total_blocks += function.body.blocks.len();
                    total_statements += function.body.statements.len();
                    total_locals += function
                        .body
                        .statements
                        .iter()
                        .filter(|statement| {
                            matches!(
                                &statement.kind,
                                syntax::RawStatementKind::LocalDeclaration { .. }
                            )
                        })
                        .count();
                    total_expressions += function.body.expressions.len();
                    functions.push(function);
                }
                _ => return Err(parser.function_error_here("unsupported top-level syntax")),
            }
        }
        files.push(syntax::RawSourceUnit {
            id: file.id().index(),
            path: file.path().as_str().to_owned(),
            imports,
            functions,
        });
    }
    Ok(syntax::RawProjectSyntaxSnapshot {
        schema_version: syntax::PROTOCOL_VERSION,
        files,
        diagnostics: Vec::new(),
    })
}

impl FileParser<'_> {
    fn function_error_here(&self, message: &'static str) -> ParseError {
        self.current()
            .or_else(|| self.tokens.last().copied())
            .map_or_else(|| failure(message), |token| function_error_at(token, message))
    }

    fn function_take(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        let token =
            self.current().ok_or_else(|| self.function_error_here("incomplete function"))?;
        if token.kind() != kind {
            return Err(self.function_error_here("unsupported straight-line function syntax"));
        }
        self.position += 1;
        Ok(token)
    }

    fn function_identifier(&mut self) -> Result<syntax::RawIdentifierSyntax, ParseError> {
        let token = self.function_take(TokenKind::Identifier)?;
        Ok(syntax::RawIdentifierSyntax { text: self.spelling(token).to_owned(), span: raw(token) })
    }

    fn named_type(&mut self) -> Result<syntax::RawTypeSyntax, ParseError> {
        self.function_take(TokenKind::Colon)?;
        let token = self.function_take(TokenKind::Identifier)?;
        let name = self.spelling(token);
        if !matches!(name, "i32" | "bool") {
            return Err(function_error_at(token, "unsupported type annotation"));
        }
        Ok(syntax::RawTypeSyntax {
            span: raw(token),
            kind: syntax::RawTypeSyntaxKind::Named { name: name.to_owned() },
        })
    }

    fn local_declaration(
        &mut self,
        keyword: Token,
        mutable: bool,
        expressions: &mut Vec<syntax::RawExpressionSyntax>,
        previous_expressions: usize,
    ) -> Result<syntax::RawStatementSyntax, ParseError> {
        self.position += 1;
        let name = self.function_identifier()?;
        let type_syntax = self.named_type()?;
        let equals = self.function_take(TokenKind::Equals)?;
        let initializer = self.expression(expressions, previous_expressions)?;
        let semicolon = self.function_take(TokenKind::Semicolon)?;
        Ok(syntax::RawStatementSyntax {
            span: UntrustedSpan {
                file: self.file,
                start: keyword.span().start(),
                end: semicolon.span().end(),
            },
            kind: syntax::RawStatementKind::LocalDeclaration {
                keyword_span: raw(keyword),
                mutable,
                name,
                type_syntax,
                equals_span: raw(equals),
                initializer,
                semicolon_span: raw(semicolon),
            },
        })
    }

    fn assignment(
        &mut self,
        expressions: &mut Vec<syntax::RawExpressionSyntax>,
        previous_expressions: usize,
    ) -> Result<syntax::RawStatementSyntax, ParseError> {
        let target = self.function_identifier()?;
        let equals = self.function_take(TokenKind::Equals)?;
        let value = self.expression(expressions, previous_expressions)?;
        let semicolon = self.function_take(TokenKind::Semicolon)?;
        Ok(syntax::RawStatementSyntax {
            span: UntrustedSpan {
                file: self.file,
                start: target.span.start,
                end: semicolon.span().end(),
            },
            kind: syntax::RawStatementKind::Assignment {
                target,
                equals_span: raw(equals),
                value,
                semicolon_span: raw(semicolon),
            },
        })
    }

    fn final_return(
        &mut self,
        expressions: &mut Vec<syntax::RawExpressionSyntax>,
        previous_expressions: usize,
    ) -> Result<(syntax::RawStatementSyntax, Token), ParseError> {
        let keyword = self.function_take(TokenKind::Keyword(Keyword::Return))?;
        let value = self.expression(expressions, previous_expressions)?;
        let semicolon = self.function_take(TokenKind::Semicolon)?;
        let close = self.function_take(TokenKind::CloseBrace)?;
        let statement = syntax::RawStatementSyntax {
            span: UntrustedSpan {
                file: self.file,
                start: keyword.span().start(),
                end: semicolon.span().end(),
            },
            kind: syntax::RawStatementKind::Return {
                keyword_span: raw(keyword),
                value,
                semicolon_span: raw(semicolon),
            },
        };
        Ok((statement, close))
    }

    fn function(
        &mut self,
        previous_parameters: usize,
        previous_blocks: usize,
        previous_statements: usize,
        previous_locals: usize,
        previous_expressions: usize,
    ) -> Result<syntax::RawFunctionSyntax, ParseError> {
        let export = self.maybe(TokenKind::Keyword(Keyword::Export));
        let keyword = self.function_take(TokenKind::Keyword(Keyword::Function))?;
        let name = self.function_identifier()?;
        self.function_take(TokenKind::OpenParen)?;
        let mut parameters = Vec::new();
        if self.current().is_some_and(|token| token.kind() != TokenKind::CloseParen) {
            loop {
                if parameters.len() >= syntax::MAX_PARAMETERS_PER_FUNCTION
                    || previous_parameters + parameters.len() >= syntax::MAX_PARAMETERS_PER_PROJECT
                {
                    return Err(resource("parameter inventory exceeds protocol-v3 limit"));
                }
                let name = self.function_identifier()?;
                let type_syntax = self.named_type()?;
                parameters.push(syntax::RawParameterSyntax {
                    span: UntrustedSpan {
                        file: self.file,
                        start: name.span.start,
                        end: type_syntax.span.end,
                    },
                    name,
                    type_syntax,
                });
                if self.maybe(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.function_take(TokenKind::CloseParen)?;
        let result_type = self.named_type()?;
        let open = self.function_take(TokenKind::OpenBrace)?;
        let (body, close) = self.body(
            open,
            previous_blocks,
            previous_statements,
            previous_locals,
            previous_expressions,
        )?;
        Ok(syntax::RawFunctionSyntax {
            span: UntrustedSpan {
                file: self.file,
                start: export.unwrap_or(keyword).span().start(),
                end: close.span().end(),
            },
            export_span: export.map(raw),
            function_span: raw(keyword),
            name,
            parameters,
            result_type,
            body,
        })
    }
}

fn failure(message: &'static str) -> ParseError {
    ParseError {
        diagnostic: Diagnostic::error(
            "ZRYNA-F2002",
            None,
            message,
            "use the supported straight-line protocol-v3 syntax",
        ),
    }
}

fn function_error_at(token: Token, message: &'static str) -> ParseError {
    ParseError {
        diagnostic: Diagnostic::error_at(
            "ZRYNA-F2002",
            token.span(),
            message,
            "use the supported straight-line protocol-v3 syntax",
        ),
    }
}

fn resource(message: &'static str) -> ParseError {
    ParseError {
        diagnostic: Diagnostic::error(
            "ZRYNA-F1002",
            None,
            message,
            "keep the candidate within protocol-v3 resource limits",
        ),
    }
}
