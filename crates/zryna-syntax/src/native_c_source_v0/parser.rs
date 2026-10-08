use std::collections::BTreeSet;

use super::{
    MAX_FUNCTIONS, MAX_STATEMENTS, SourceAuthError, error,
    lexer::{Kind, Token, lex},
    limit,
    raw::{self, ExpressionKind, StatementKind},
};
use crate::native_c_v0::raw::Primitive;

mod expression;

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    cursor: usize,
    expressions: Vec<raw::Expression>,
    depths: Vec<usize>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Token<'a> {
        self.tokens[self.cursor]
    }
    fn take(&mut self, text: &str) -> Result<Token<'a>, SourceAuthError> {
        let token = self.peek();
        if token.text != text {
            return Err(error("expected-token", token.range));
        }
        self.cursor += 1;
        Ok(token)
    }
    fn name(&mut self) -> Result<Token<'a>, SourceAuthError> {
        let token = self.peek();
        if token.kind != Kind::Name
            || matches!(
                token.text,
                "Ffi"
                    | "function"
                    | "export"
                    | "const"
                    | "return"
                    | "if"
                    | "true"
                    | "false"
                    | "import"
                    | "let"
                    | "var"
                    | "class"
                    | "new"
                    | "null"
                    | "this"
                    | "throw"
                    | "try"
                    | "while"
                    | "for"
                    | "switch"
                    | "else"
            )
        {
            return Err(error("binding-name", token.range));
        }
        self.cursor += 1;
        Ok(token)
    }
    fn ty(&mut self) -> Result<raw::Type, SourceAuthError> {
        use raw::Type::{
            Bool, Bytes, BytesOut, CountOut, Handle, HandleOut, I32, I32Out, OwnedBytes, String,
            VecI32,
        };
        let token = self.peek();
        self.cursor += 1;
        Ok(match token.text {
            "i32" => I32,
            "bool" => Bool,
            "String" => String,
            "Vec" => {
                self.take("<")?;
                self.take("i32")?;
                self.take(">")?;
                VecI32
            }
            "FfiBytes" => Bytes,
            "FfiI32Out" => I32Out,
            "FfiHandleOut" => HandleOut,
            "FfiBytesOut" => BytesOut,
            "FfiCountOut" => CountOut,
            "FfiHandle" => Handle,
            "FfiOwnedBytes" => OwnedBytes,
            _ => return Err(error("type", token.range)),
        })
    }
    fn binding(&mut self) -> Result<raw::Binding, SourceAuthError> {
        let name = self.name()?;
        self.take(":")?;
        let ty = self.ty()?;
        Ok(raw::Binding {
            name: name.text.to_owned(),
            ty,
            range: raw::Range {
                start: name.range.start,
                end: self.tokens[self.cursor - 1].range.end,
            },
        })
    }
    fn statement(&mut self) -> Result<raw::Statement, SourceAuthError> {
        let start = self.peek().range.start;
        let kind = match self.peek().text {
            "const" => {
                self.take("const")?;
                let binding = self.binding()?;
                self.take("=")?;
                StatementKind::Const(binding, self.expression(1)?)
            }
            "return" => {
                self.take("return")?;
                StatementKind::Return(self.expression(1)?)
            }
            "if" => {
                self.take("if")?;
                self.take("(")?;
                let status = self.name()?.text.to_owned();
                self.take("!==")?;
                self.take("0")?;
                self.take(")")?;
                self.take("{")?;
                self.take("return")?;
                let expression = self.expression(1)?;
                if !matches!(
                    self.expressions[expression].kind,
                    ExpressionKind::Intrinsic(Primitive::ForeignError, _)
                ) {
                    return Err(error("terminal-status-guard", self.expressions[expression].range));
                }
                self.take(";")?;
                let end = self.take("}")?.range.end;
                return Ok(raw::Statement {
                    range: raw::Range { start, end },
                    kind: StatementKind::Guard(status, expression),
                });
            }
            _ => StatementKind::Expression(self.expression(1)?),
        };
        let end = self.take(";")?.range.end;
        Ok(raw::Statement { range: raw::Range { start, end }, kind })
    }
    fn function(&mut self) -> Result<raw::Function, SourceAuthError> {
        self.expressions.clear();
        self.depths.clear();
        let start = self.peek().range.start;
        let exported = self.peek().text == "export";
        if exported {
            self.take("export")?;
        }
        self.take("function")?;
        let name = self.name()?.text.to_owned();
        self.take("(")?;
        let mut parameters = Vec::new();
        if self.peek().text != ")" {
            loop {
                if parameters.len() == 16 {
                    return Err(limit("parameters", self.peek().range));
                }
                parameters.push(self.binding()?);
                if self.peek().text != "," {
                    break;
                }
                self.take(",")?;
            }
        }
        self.take(")")?;
        self.take(":")?;
        let result = self.ty()?;
        self.take("{")?;
        let mut statements = Vec::new();
        while self.peek().text != "}" {
            if self.peek().kind == Kind::End {
                return Err(error("unterminated-function", self.peek().range));
            }
            if statements.len() == MAX_STATEMENTS {
                return Err(limit("statements", self.peek().range));
            }
            statements.push(self.statement()?);
        }
        let end = self.take("}")?.range.end;
        Ok(raw::Function {
            range: raw::Range { start, end },
            name,
            exported,
            parameters,
            result,
            statements,
            expressions: std::mem::take(&mut self.expressions),
        })
    }
}

pub(super) fn parse(text: &str) -> Result<Vec<raw::Function>, SourceAuthError> {
    let mut parser =
        Parser { tokens: lex(text)?, cursor: 0, expressions: Vec::new(), depths: Vec::new() };
    let mut functions = Vec::new();
    let mut names = BTreeSet::new();
    while parser.peek().kind != Kind::End {
        if functions.len() == MAX_FUNCTIONS {
            return Err(limit("functions", parser.peek().range));
        }
        let function = parser.function()?;
        if !names.insert(function.name.clone()) {
            return Err(error("duplicate-function", function.range));
        }
        functions.push(function);
    }
    Ok(functions)
}
