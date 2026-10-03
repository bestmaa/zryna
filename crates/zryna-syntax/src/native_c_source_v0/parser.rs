use std::collections::BTreeSet;

use super::{
    MAX_EXPRESSION_DEPTH, MAX_EXPRESSIONS, MAX_FUNCTIONS, MAX_STATEMENTS, SourceAuthError, error,
    lexer::{Kind, Token, lex},
    limit,
    raw::{self, ExpressionKind, StatementKind},
};
use crate::native_c_v0::raw::Primitive;

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    cursor: usize,
    expressions: Vec<raw::Expression>,
    depths: Vec<usize>,
}

fn primitive(text: &str) -> Option<(Primitive, Option<usize>)> {
    use Primitive::{
        BorrowBytes, BorrowUtf8, ByteLength, CopyBytes, ForeignError, OutBytes, OutCount,
        OutHandle, OutI32, RawCall, ReadI32, Release, TakeBytes, TakeHandle,
    };
    Some(match text {
        "rawCall" => (RawCall, None),
        "borrowBytes" => (BorrowBytes, Some(1)),
        "borrowUtf8" => (BorrowUtf8, Some(1)),
        "byteLength" => (ByteLength, Some(1)),
        "outI32" => (OutI32, Some(0)),
        "outHandle" => (OutHandle, Some(1)),
        "outBytes" => (OutBytes, Some(0)),
        "outCount" => (OutCount, Some(0)),
        "readI32" => (ReadI32, Some(1)),
        "takeHandle" => (TakeHandle, Some(1)),
        "takeBytes" => (TakeBytes, Some(2)),
        "copyBytes" => (CopyBytes, Some(1)),
        "release" => (Release, Some(2)),
        "foreignError" => (ForeignError, Some(2)),
        _ => return None,
    })
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
    fn push(
        &mut self,
        range: raw::Range,
        kind: ExpressionKind,
        depth: usize,
    ) -> Result<usize, SourceAuthError> {
        if depth > MAX_EXPRESSION_DEPTH {
            return Err(limit("expression-depth", range));
        }
        if self.expressions.len() == MAX_EXPRESSIONS {
            return Err(limit("expressions", range));
        }
        let index = self.expressions.len();
        self.expressions.push(raw::Expression { range, kind });
        self.depths.push(depth);
        Ok(index)
    }
    fn expression(&mut self, nesting: usize) -> Result<usize, SourceAuthError> {
        if nesting > MAX_EXPRESSION_DEPTH {
            return Err(limit("expression-depth", self.peek().range));
        }
        let mut left = self.atom(nesting)?;
        while self.peek().text == "+" {
            self.take("+")?;
            let right = self.atom(nesting)?;
            let range = raw::Range {
                start: self.expressions[left].range.start,
                end: self.expressions[right].range.end,
            };
            let depth = 1 + self.depths[left].max(self.depths[right]);
            left = self.push(range, ExpressionKind::Add(left, right), depth)?;
        }
        Ok(left)
    }
    fn atom(&mut self, nesting: usize) -> Result<usize, SourceAuthError> {
        let token = self.peek();
        let start = token.range.start;
        if token.text == "Ffi" {
            self.take("Ffi")?;
            self.take(".")?;
            let tag = self.peek();
            let (primitive, arity) =
                primitive(tag.text).ok_or_else(|| error("primitive", tag.range))?;
            self.cursor += 1;
            self.take("(")?;
            let mut args = Vec::new();
            if self.peek().text != ")" {
                loop {
                    if args.len() == 17 {
                        return Err(limit("argument-count", self.peek().range));
                    }
                    args.push(self.expression(nesting + 1)?);
                    if self.peek().text != "," {
                        break;
                    }
                    self.take(",")?;
                }
            }
            let end = self.take(")")?.range.end;
            let range = raw::Range { start, end };
            if arity.is_some_and(|expected| args.len() != expected)
                || (arity.is_none() && args.is_empty())
            {
                return Err(error("primitive-arity", range));
            }
            let needs_key = matches!(
                primitive,
                Primitive::RawCall
                    | Primitive::Release
                    | Primitive::ForeignError
                    | Primitive::OutHandle
            );
            if needs_key
                && !matches!(
                    args.first().map(|index| &self.expressions[*index].kind),
                    Some(ExpressionKind::Key(_))
                )
            {
                return Err(error("literal-key", range));
            }
            let depth = 1 + args.iter().map(|index| self.depths[*index]).max().unwrap_or(0);
            return self.push(range, ExpressionKind::Intrinsic(primitive, args), depth);
        }
        if token.kind == Kind::Key {
            self.cursor += 1;
            let key = &token.text[1..token.text.len() - 1];
            if key.is_empty()
                || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b"_/@.-".contains(&b))
            {
                return Err(error("key", token.range));
            }
            return self.push(token.range, ExpressionKind::Key(key.to_owned()), 1);
        }
        if token.kind == Kind::Integer || token.text == "-" {
            let negative = token.text == "-";
            if negative {
                self.take("-")?;
            }
            let number = self.peek();
            if number.kind != Kind::Integer
                || (number.text.len() > 1 && number.text.starts_with('0'))
            {
                return Err(error("integer", number.range));
            }
            self.cursor += 1;
            let magnitude: i64 =
                number.text.parse().map_err(|_| error("i32-range", number.range))?;
            let value = if negative { -magnitude } else { magnitude };
            let value = i32::try_from(value).map_err(|_| SourceAuthError {
                code: "ZRYNA-C4104",
                detail: "i32-range",
                range: number.range,
            })?;
            return self.push(
                raw::Range { start, end: number.range.end },
                ExpressionKind::I32(value),
                1,
            );
        }
        if matches!(token.text, "true" | "false") {
            self.cursor += 1;
            return self.push(token.range, ExpressionKind::Bool(token.text == "true"), 1);
        }
        let name = self.name()?;
        self.push(name.range, ExpressionKind::Local(name.text.to_owned()), 1)
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
