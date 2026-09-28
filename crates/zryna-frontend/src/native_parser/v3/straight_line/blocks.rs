//! Bounded preorder construction for standalone protocol-v3 lexical blocks.

use zryna_source::UntrustedSpan;
use zryna_syntax::v3 as syntax;

use crate::native_lexer::{Keyword, Token, TokenKind};

use super::{FileParser, ParseError, raw, resource};

struct Frame {
    block: usize,
    open: Token,
    owner_statement: Option<usize>,
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

fn finish_block(
    frame: Frame,
    close: Token,
    file: u32,
    blocks: &mut [syntax::RawBlockSyntax],
    statements: &mut [syntax::RawStatementSyntax],
) {
    let span = UntrustedSpan { file, start: frame.open.span().start(), end: close.span().end() };
    let block = &mut blocks[frame.block];
    block.span = span;
    block.statements = frame.statements;
    block.close_brace_span = raw(close);
    if let Some(owner) = frame.owner_statement {
        statements[owner].span = span;
    }
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
        let mut frames =
            vec![Frame { block: 0, open, owner_statement: None, statements: Vec::new() }];
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
                    )?;
                    frames.last_mut().expect("root frame").statements.push(
                        u32::try_from(statements.len()).expect("bounded statement inventory"),
                    );
                    statements.push(statement);
                    locals += 1;
                }
                TokenKind::Identifier => {
                    Self::statement_room(&statements, previous_statements)?;
                    let statement = self.assignment(&mut expressions, previous_expressions)?;
                    frames.last_mut().expect("root frame").statements.push(
                        u32::try_from(statements.len()).expect("bounded statement inventory"),
                    );
                    statements.push(statement);
                }
                TokenKind::OpenBrace => {
                    Self::statement_room(&statements, previous_statements)?;
                    Self::block_room(&blocks, previous_blocks)?;
                    if frames.len() >= syntax::MAX_NESTING_DEPTH as usize {
                        return Err(resource("block nesting exceeds protocol-v3 limit"));
                    }
                    let open = self.function_take(TokenKind::OpenBrace)?;
                    let block = blocks.len();
                    let owner_statement = statements.len();
                    frames
                        .last_mut()
                        .expect("root frame")
                        .statements
                        .push(u32::try_from(owner_statement).expect("bounded statement inventory"));
                    statements.push(syntax::RawStatementSyntax {
                        span: raw(open),
                        kind: syntax::RawStatementKind::Block {
                            block: u32::try_from(block).expect("bounded block inventory"),
                        },
                    });
                    blocks.push(unfinished_block(open));
                    frames.push(Frame {
                        block,
                        open,
                        owner_statement: Some(owner_statement),
                        statements: Vec::new(),
                    });
                }
                TokenKind::CloseBrace if frames.len() > 1 => {
                    let close = self.function_take(TokenKind::CloseBrace)?;
                    let frame = frames.pop().expect("child frame");
                    finish_block(frame, close, self.file, &mut blocks, &mut statements);
                }
                TokenKind::Keyword(Keyword::Return) if frames.len() == 1 => {
                    Self::statement_room(&statements, previous_statements)?;
                    let (statement, close) =
                        self.final_return(&mut expressions, previous_expressions)?;
                    frames.last_mut().expect("root frame").statements.push(
                        u32::try_from(statements.len()).expect("bounded statement inventory"),
                    );
                    statements.push(statement);
                    let root = frames.pop().expect("root frame");
                    finish_block(root, close, self.file, &mut blocks, &mut statements);
                    return Ok((
                        syntax::RawFunctionBodySyntax {
                            span: blocks[0].span,
                            root_block: 0,
                            blocks,
                            statements,
                            expressions,
                        },
                        close,
                    ));
                }
                _ => return Err(self.function_error_here("unsupported block statement")),
            }
        }
    }
}
