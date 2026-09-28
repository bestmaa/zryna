//! Source-ordered function arenas for the native protocol-v4 candidate.

use zryna_source::UntrustedSpan;
use zryna_syntax::v4 as syntax;

use crate::native_lexer::{Keyword, Token, TokenKind};

use super::{FileParser, ParseError, raw, resource, unsupported};

mod constructions;
mod expression;

struct Body {
    blocks: Vec<syntax::RawBlockSyntax>,
    statements: Vec<syntax::RawStatementSyntax>,
    expressions: Vec<syntax::RawExpressionSyntax>,
    locals: usize,
}

#[derive(Clone, Copy)]
enum Owner {
    Standalone(usize),
    IfThen(usize),
    IfElse(usize),
    While(usize),
    WeakSuccess(usize),
    WeakFailure(usize),
}

impl Owner {
    fn statement(self) -> usize {
        match self {
            Self::Standalone(index)
            | Self::IfThen(index)
            | Self::IfElse(index)
            | Self::While(index)
            | Self::WeakSuccess(index)
            | Self::WeakFailure(index) => index,
        }
    }
}

struct Frame {
    block: usize,
    open: Token,
    owner: Option<Owner>,
    statements: Vec<u32>,
}

fn unfinished_block(open: Token) -> syntax::RawBlockSyntax {
    syntax::RawBlockSyntax {
        span: raw(open),
        open_brace_span: raw(open),
        statements: Vec::new(),
        close_brace_span: raw(open),
    }
}

impl FileParser<'_> {
    pub(super) fn function(&mut self) -> Result<syntax::RawFunctionSyntax, ParseError> {
        let export = self.maybe(TokenKind::Keyword(Keyword::Export));
        let keyword = self.take(TokenKind::Keyword(Keyword::Function))?;
        let name = self.identifier()?;
        self.take(TokenKind::OpenParen)?;
        let mut parameters = Vec::new();
        while self.current().is_some_and(|token| token.kind() != TokenKind::CloseParen) {
            if parameters.len() >= syntax::MAX_PARAMETERS_PER_FUNCTION
                || self.previous_parameters + parameters.len() >= syntax::MAX_PARAMETERS_PER_PROJECT
            {
                return Err(resource("parameter inventory exceeds protocol-v4 limit"));
            }
            let name = self.identifier()?;
            let type_syntax = self.optional_type(name.span.end)?;
            let end = self.types[type_syntax as usize].span.end;
            parameters.push(syntax::RawParameterSyntax {
                span: UntrustedSpan { file: self.file, start: name.span.start, end },
                name,
                type_syntax,
            });
            if self.maybe(TokenKind::Comma).is_none() {
                break;
            }
        }
        let close_paren = self.take(TokenKind::CloseParen)?;
        let result_type = self.optional_type(close_paren.span().start())?;
        let open = self.take(TokenKind::OpenBrace)?;
        let (body, close) = self.body(open)?;
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

    fn body(&mut self, open: Token) -> Result<(syntax::RawFunctionBodySyntax, Token), ParseError> {
        if self.previous_blocks >= syntax::MAX_BLOCKS_PER_PROJECT {
            return Err(resource("block inventory exceeds protocol-v4 limit"));
        }
        let mut body = Body {
            blocks: vec![unfinished_block(open)],
            statements: Vec::new(),
            expressions: Vec::new(),
            locals: 0,
        };
        let mut frames = vec![Frame { block: 0, open, owner: None, statements: Vec::new() }];
        loop {
            let token = self.current().ok_or_else(|| unsupported(None, "incomplete block"))?;
            if token.kind() == TokenKind::CloseBrace {
                if let Some(close) = self.close_frame(&mut body, &mut frames)? {
                    let root_span = body.blocks[0].span;
                    return Ok((
                        syntax::RawFunctionBodySyntax {
                            span: root_span,
                            root_block: 0,
                            blocks: body.blocks,
                            statements: body.statements,
                            expressions: body.expressions,
                        },
                        close,
                    ));
                }
                continue;
            }
            if body.statements.len() >= syntax::MAX_STATEMENTS_PER_FUNCTION
                || self.previous_statements + body.statements.len()
                    >= syntax::MAX_STATEMENTS_PER_PROJECT
            {
                return Err(resource("statement inventory exceeds protocol-v4 limit"));
            }
            let id = body.statements.len();
            frames
                .last_mut()
                .expect("root frame")
                .statements
                .push(u32::try_from(id).expect("bounded statements"));
            match token.kind() {
                TokenKind::OpenBrace => {
                    body.statements.push(syntax::RawStatementSyntax {
                        span: raw(token),
                        kind: syntax::RawStatementKind::Block {
                            block: u32::try_from(body.blocks.len()).expect("bounded blocks"),
                        },
                    });
                    self.open_child(&mut body, &mut frames, Owner::Standalone(id))?;
                }
                TokenKind::Keyword(Keyword::If | Keyword::While) => {
                    self.position += 1;
                    let open_paren = self.take(TokenKind::OpenParen)?;
                    let condition = self.expression(&mut body)?;
                    let close_paren = self.take(TokenKind::CloseParen)?;
                    let block = u32::try_from(body.blocks.len()).expect("bounded blocks");
                    let is_if = token.kind() == TokenKind::Keyword(Keyword::If);
                    let kind = if is_if {
                        syntax::RawStatementKind::If {
                            keyword_span: raw(token),
                            open_paren_span: raw(open_paren),
                            condition,
                            close_paren_span: raw(close_paren),
                            then_block: block,
                            else_clause: None,
                        }
                    } else {
                        syntax::RawStatementKind::While {
                            keyword_span: raw(token),
                            open_paren_span: raw(open_paren),
                            condition,
                            close_paren_span: raw(close_paren),
                            body_block: block,
                        }
                    };
                    body.statements.push(syntax::RawStatementSyntax {
                        span: UntrustedSpan {
                            file: self.file,
                            start: token.span().start(),
                            end: close_paren.span().end(),
                        },
                        kind,
                    });
                    self.open_child(
                        &mut body,
                        &mut frames,
                        if is_if { Owner::IfThen(id) } else { Owner::While(id) },
                    )?;
                }
                TokenKind::Identifier if self.spelling(token) == "upgradeWeak" => {
                    self.weak_upgrade(&mut body, &mut frames, token, id)?;
                }
                _ => {
                    let statement = self.statement(&mut body)?;
                    body.statements.push(statement);
                }
            }
        }
    }

    fn close_frame(
        &mut self,
        body: &mut Body,
        frames: &mut Vec<Frame>,
    ) -> Result<Option<Token>, ParseError> {
        let close = self.take(TokenKind::CloseBrace)?;
        let frame = frames.pop().expect("open block frame");
        let block_span = UntrustedSpan {
            file: self.file,
            start: frame.open.span().start(),
            end: close.span().end(),
        };
        let block = &mut body.blocks[frame.block];
        block.span = block_span;
        block.close_brace_span = raw(close);
        block.statements = frame.statements;
        if let Some(owner) = frame.owner {
            body.statements[owner.statement()].span.end = block_span.end;
        }
        if frames.is_empty() {
            return Ok(Some(close));
        }
        if let Some(Owner::WeakSuccess(index)) = frame.owner {
            self.take(TokenKind::Comma)?;
            self.take(TokenKind::OpenParen)?;
            self.take(TokenKind::CloseParen)?;
            let arrow = self.take(TokenKind::FatArrow)?;
            let failure = u32::try_from(body.blocks.len()).expect("bounded blocks");
            let syntax::RawStatementKind::WeakUpgrade { else_span, failure_block, .. } =
                &mut body.statements[index].kind
            else {
                unreachable!("weak owner")
            };
            *else_span = raw(arrow);
            *failure_block = failure;
            self.open_child(body, frames, Owner::WeakFailure(index))?;
        }
        if let Some(Owner::WeakFailure(index)) = frame.owner {
            self.take(TokenKind::CloseParen)?;
            let semicolon = self.take(TokenKind::Semicolon)?;
            body.statements[index].span.end = semicolon.span().end();
        }
        if let Some(Owner::IfThen(index)) = frame.owner
            && let Some(else_token) = self.maybe(TokenKind::Keyword(Keyword::Else))
        {
            let block = u32::try_from(body.blocks.len()).expect("bounded blocks");
            let syntax::RawStatementKind::If { else_clause, .. } = &mut body.statements[index].kind
            else {
                unreachable!("if owner")
            };
            *else_clause = Some(syntax::RawElseSyntax { keyword_span: raw(else_token), block });
            self.open_child(body, frames, Owner::IfElse(index))?;
        }
        Ok(None)
    }

    fn weak_upgrade(
        &mut self,
        body: &mut Body,
        frames: &mut Vec<Frame>,
        keyword: Token,
        statement: usize,
    ) -> Result<(), ParseError> {
        self.position += 1;
        self.take(TokenKind::OpenParen)?;
        let weak = self.expression(body)?;
        self.take(TokenKind::Comma)?;
        self.take(TokenKind::OpenParen)?;
        let binding = self.identifier()?;
        self.take(TokenKind::CloseParen)?;
        let arrow = self.take(TokenKind::FatArrow)?;
        let success_block = u32::try_from(body.blocks.len()).expect("bounded blocks");
        body.statements.push(syntax::RawStatementSyntax {
            span: UntrustedSpan {
                file: self.file,
                start: keyword.span().start(),
                end: arrow.span().end(),
            },
            kind: syntax::RawStatementKind::WeakUpgrade {
                keyword_span: raw(keyword),
                weak,
                as_span: raw(arrow),
                binding,
                success_block,
                else_span: raw(arrow),
                failure_block: 0,
            },
        });
        self.open_child(body, frames, Owner::WeakSuccess(statement))
    }

    fn open_child(
        &mut self,
        body: &mut Body,
        frames: &mut Vec<Frame>,
        owner: Owner,
    ) -> Result<(), ParseError> {
        if body.blocks.len() >= syntax::MAX_BLOCKS_PER_FUNCTION
            || self.previous_blocks + body.blocks.len() >= syntax::MAX_BLOCKS_PER_PROJECT
        {
            return Err(resource("block inventory exceeds protocol-v4 limit"));
        }
        if frames.len() >= syntax::MAX_NESTING_DEPTH as usize {
            return Err(resource("block nesting exceeds protocol-v4 limit"));
        }
        let open = self.take(TokenKind::OpenBrace)?;
        let block = body.blocks.len();
        body.blocks.push(unfinished_block(open));
        frames.push(Frame { block, open, owner: Some(owner), statements: Vec::new() });
        Ok(())
    }

    fn statement(&mut self, body: &mut Body) -> Result<syntax::RawStatementSyntax, ParseError> {
        let first = self.current().ok_or_else(|| unsupported(None, "missing statement"))?;
        let kind = match first.kind() {
            TokenKind::Keyword(Keyword::Return) => {
                self.position += 1;
                let value = self.expression(body)?;
                let semicolon = self.take(TokenKind::Semicolon)?;
                syntax::RawStatementKind::Return {
                    keyword_span: raw(first),
                    value,
                    semicolon_span: raw(semicolon),
                }
            }
            TokenKind::Keyword(Keyword::Const | Keyword::Let) => {
                if body.locals >= 4_096 || self.previous_locals + body.locals >= 65_536 {
                    return Err(resource("local inventory exceeds protocol-v4 limit"));
                }
                body.locals += 1;
                self.position += 1;
                let name = self.identifier()?;
                self.take(TokenKind::Colon)?;
                let type_syntax = self.type_syntax()?;
                let equals = self.take(TokenKind::Equals)?;
                let initializer = self.expression(body)?;
                let semicolon = self.take(TokenKind::Semicolon)?;
                syntax::RawStatementKind::LocalDeclaration {
                    keyword_span: raw(first),
                    mutable: first.kind() == TokenKind::Keyword(Keyword::Let),
                    name,
                    type_syntax,
                    equals_span: raw(equals),
                    initializer,
                    semicolon_span: raw(semicolon),
                }
            }
            _ => {
                let target = self.expression(body)?;
                if let Some(equals) = self.maybe(TokenKind::Equals) {
                    let value = self.expression(body)?;
                    let semicolon = self.take(TokenKind::Semicolon)?;
                    syntax::RawStatementKind::Assignment {
                        target,
                        equals_span: raw(equals),
                        value,
                        semicolon_span: raw(semicolon),
                    }
                } else {
                    let semicolon = self.take(TokenKind::Semicolon)?;
                    syntax::RawStatementKind::ExpressionStatement {
                        expression: target,
                        semicolon_span: raw(semicolon),
                    }
                }
            }
        };
        let end = self.tokens[self.position - 1].span().end();
        Ok(syntax::RawStatementSyntax {
            span: UntrustedSpan { file: self.file, start: first.span().start(), end },
            kind,
        })
    }
}
