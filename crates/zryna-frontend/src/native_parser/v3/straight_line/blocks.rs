//! Bounded preorder construction for protocol-v3 structured statements.

use zryna_source::UntrustedSpan;
use zryna_syntax::v3 as syntax;

use crate::native_lexer::{Keyword, Token, TokenKind};

use super::{FileParser, ParseError, raw, resource};

#[derive(Clone, Copy)]
enum Owner {
    Standalone(usize),
    IfThen(usize),
    IfElse(usize),
    While(usize),
}

impl Owner {
    fn statement(self) -> usize {
        match self {
            Self::Standalone(index)
            | Self::IfThen(index)
            | Self::IfElse(index)
            | Self::While(index) => index,
        }
    }
}

struct Frame {
    block: usize,
    open: Token,
    owner: Option<Owner>,
    statements: Vec<u32>,
}

struct ControlArenas<'a> {
    frames: &'a mut Vec<Frame>,
    blocks: &'a mut Vec<syntax::RawBlockSyntax>,
    statements: &'a mut Vec<syntax::RawStatementSyntax>,
    expressions: &'a mut Vec<syntax::RawExpressionSyntax>,
    previous_blocks: usize,
    previous_statements: usize,
    previous_expressions: usize,
}

fn frame_depth(frames: &[Frame]) -> u32 {
    u32::try_from(frames.len()).expect("bounded block depth")
}

fn push_statement(
    frames: &mut [Frame],
    statements: &mut Vec<syntax::RawStatementSyntax>,
    statement: syntax::RawStatementSyntax,
) {
    let id = u32::try_from(statements.len()).expect("bounded statement inventory");
    frames.last_mut().expect("root frame").statements.push(id);
    statements.push(statement);
}

fn unfinished_block(open: Token) -> syntax::RawBlockSyntax {
    syntax::RawBlockSyntax {
        span: raw(open),
        open_brace_span: raw(open),
        statements: Vec::new(),
        close_brace_span: raw(open),
    }
}

fn completed_body(
    blocks: Vec<syntax::RawBlockSyntax>,
    statements: Vec<syntax::RawStatementSyntax>,
    expressions: Vec<syntax::RawExpressionSyntax>,
) -> syntax::RawFunctionBodySyntax {
    syntax::RawFunctionBodySyntax {
        span: blocks[0].span,
        root_block: 0,
        blocks,
        statements,
        expressions,
    }
}

fn finish_block(
    frame: Frame,
    close: Token,
    file: u32,
    blocks: &mut [syntax::RawBlockSyntax],
    statements: &mut [syntax::RawStatementSyntax],
) -> Option<Owner> {
    let span = UntrustedSpan { file, start: frame.open.span().start(), end: close.span().end() };
    let block = &mut blocks[frame.block];
    block.span = span;
    block.statements = frame.statements;
    block.close_brace_span = raw(close);
    if let Some(owner) = frame.owner {
        statements[owner.statement()].span.end = span.end;
    }
    frame.owner
}

impl FileParser<'_> {
    fn statement_room(
        statements: &[syntax::RawStatementSyntax],
        previous_statements: usize,
    ) -> Result<(), ParseError> {
        if statements.len() >= syntax::MAX_STATEMENTS_PER_FUNCTION
            || previous_statements + statements.len() >= syntax::MAX_STATEMENTS_PER_PROJECT
        {
            return Err(resource("statement inventory exceeds protocol-v3 limit"));
        }
        Ok(())
    }

    fn block_room(
        blocks: &[syntax::RawBlockSyntax],
        previous_blocks: usize,
    ) -> Result<(), ParseError> {
        if blocks.len() >= syntax::MAX_BLOCKS_PER_FUNCTION
            || previous_blocks + blocks.len() >= syntax::MAX_BLOCKS_PER_PROJECT
        {
            return Err(resource("block inventory exceeds protocol-v3 limit"));
        }
        Ok(())
    }

    fn open_child(
        &mut self,
        frames: &mut Vec<Frame>,
        blocks: &mut Vec<syntax::RawBlockSyntax>,
        previous_blocks: usize,
        owner: Owner,
    ) -> Result<u32, ParseError> {
        Self::block_room(blocks, previous_blocks)?;
        if frames.len() >= syntax::MAX_NESTING_DEPTH as usize {
            return Err(resource("block nesting exceeds protocol-v3 limit"));
        }
        let open = self.function_take(TokenKind::OpenBrace)?;
        let id = u32::try_from(blocks.len()).expect("bounded block inventory");
        blocks.push(unfinished_block(open));
        frames.push(Frame { block: id as usize, open, owner: Some(owner), statements: Vec::new() });
        Ok(id)
    }

    fn control_statement(
        &mut self,
        keyword: Token,
        arenas: ControlArenas<'_>,
    ) -> Result<(), ParseError> {
        let ControlArenas {
            frames,
            blocks,
            statements,
            expressions,
            previous_blocks,
            previous_statements,
            previous_expressions,
        } = arenas;
        Self::statement_room(statements, previous_statements)?;
        self.position += 1;
        let open_paren = self.function_take(TokenKind::OpenParen)?;
        let condition = self.expression(expressions, previous_expressions, frame_depth(frames))?;
        let close_paren = self.function_take(TokenKind::CloseParen)?;
        let owner = statements.len();
        let child = u32::try_from(blocks.len()).expect("bounded block inventory");
        let kind = if keyword.kind() == TokenKind::Keyword(Keyword::If) {
            syntax::RawStatementKind::If {
                keyword_span: raw(keyword),
                open_paren_span: raw(open_paren),
                condition,
                close_paren_span: raw(close_paren),
                then_block: child,
                else_clause: None,
            }
        } else {
            syntax::RawStatementKind::While {
                keyword_span: raw(keyword),
                open_paren_span: raw(open_paren),
                condition,
                close_paren_span: raw(close_paren),
                body_block: child,
            }
        };
        frames
            .last_mut()
            .expect("root frame")
            .statements
            .push(u32::try_from(owner).expect("bounded statement inventory"));
        statements.push(syntax::RawStatementSyntax {
            span: UntrustedSpan {
                file: self.file,
                start: keyword.span().start(),
                end: close_paren.span().end(),
            },
            kind,
        });
        let block_owner = if keyword.kind() == TokenKind::Keyword(Keyword::If) {
            Owner::IfThen(owner)
        } else {
            Owner::While(owner)
        };
        self.open_child(frames, blocks, previous_blocks, block_owner)?;
        Ok(())
    }

    fn close_block(
        &mut self,
        frames: &mut Vec<Frame>,
        blocks: &mut Vec<syntax::RawBlockSyntax>,
        statements: &mut [syntax::RawStatementSyntax],
        previous_blocks: usize,
    ) -> Result<(Token, bool), ParseError> {
        let close = self.function_take(TokenKind::CloseBrace)?;
        let frame = frames.pop().expect("root frame");
        let owner = finish_block(frame, close, self.file, blocks, statements);
        if frames.is_empty() {
            return Ok((close, true));
        }
        if let Some(Owner::IfThen(index)) = owner
            && let Some(else_keyword) = self.maybe(TokenKind::Keyword(Keyword::Else))
        {
            let block = u32::try_from(blocks.len()).expect("bounded block inventory");
            let syntax::RawStatementKind::If { else_clause, .. } = &mut statements[index].kind
            else {
                unreachable!("if owner")
            };
            *else_clause = Some(syntax::RawElseSyntax { keyword_span: raw(else_keyword), block });
            self.open_child(frames, blocks, previous_blocks, Owner::IfElse(index))?;
        }
        Ok((close, false))
    }

    pub(super) fn body(
        &mut self,
        open: Token,
        previous_blocks: usize,
        previous_statements: usize,
        previous_locals: usize,
        previous_expressions: usize,
    ) -> Result<(syntax::RawFunctionBodySyntax, Token), ParseError> {
        Self::block_room(&[], previous_blocks)?;
        let mut blocks = vec![unfinished_block(open)];
        let mut frames = vec![Frame { block: 0, open, owner: None, statements: Vec::new() }];
        let mut statements = Vec::new();
        let mut expressions = Vec::new();
        let mut locals = 0_usize;
        loop {
            let token =
                self.current().ok_or_else(|| self.function_error_here("incomplete block"))?;
            match token.kind() {
                TokenKind::Keyword(Keyword::Let | Keyword::Const) => {
                    Self::statement_room(&statements, previous_statements)?;
                    if locals >= syntax::MAX_LOCALS_PER_FUNCTION
                        || previous_locals + locals >= syntax::MAX_LOCALS_PER_PROJECT
                    {
                        return Err(resource("local inventory exceeds protocol-v3 limit"));
                    }
                    let mutable = token.kind() == TokenKind::Keyword(Keyword::Let);
                    let statement = self.local_declaration(
                        token,
                        mutable,
                        &mut expressions,
                        previous_expressions,
                        frame_depth(&frames),
                    )?;
                    push_statement(&mut frames, &mut statements, statement);
                    locals += 1;
                }
                TokenKind::Identifier => {
                    Self::statement_room(&statements, previous_statements)?;
                    let statement = self.assignment(
                        &mut expressions,
                        previous_expressions,
                        frame_depth(&frames),
                    )?;
                    push_statement(&mut frames, &mut statements, statement);
                }
                TokenKind::Keyword(Keyword::Return) => {
                    Self::statement_room(&statements, previous_statements)?;
                    let statement = self.return_statement(
                        &mut expressions,
                        previous_expressions,
                        frame_depth(&frames),
                    )?;
                    push_statement(&mut frames, &mut statements, statement);
                }
                TokenKind::OpenBrace => {
                    Self::statement_room(&statements, previous_statements)?;
                    let owner = statements.len();
                    push_statement(
                        &mut frames,
                        &mut statements,
                        syntax::RawStatementSyntax {
                            span: raw(token),
                            kind: syntax::RawStatementKind::Block {
                                block: u32::try_from(blocks.len())
                                    .expect("bounded block inventory"),
                            },
                        },
                    );
                    self.open_child(
                        &mut frames,
                        &mut blocks,
                        previous_blocks,
                        Owner::Standalone(owner),
                    )?;
                }
                TokenKind::Keyword(Keyword::If | Keyword::While) => {
                    self.control_statement(
                        token,
                        ControlArenas {
                            frames: &mut frames,
                            blocks: &mut blocks,
                            statements: &mut statements,
                            expressions: &mut expressions,
                            previous_blocks,
                            previous_statements,
                            previous_expressions,
                        },
                    )?;
                }
                TokenKind::CloseBrace => {
                    let (close, root) = self.close_block(
                        &mut frames,
                        &mut blocks,
                        &mut statements,
                        previous_blocks,
                    )?;
                    if root {
                        return Ok((completed_body(blocks, statements, expressions), close));
                    }
                }
                _ => return Err(self.function_error_here("unsupported block statement")),
            }
        }
    }
}
