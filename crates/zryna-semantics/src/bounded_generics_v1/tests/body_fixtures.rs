//! Independent fixture reader: complete source first, raw DTO second, production v5 verification
//! last. It has no semantic type/name resolution and cannot manufacture a verified snapshot.

use serde_json::{Value, json};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::v5::{RawProjectSyntaxSnapshot, verify_snapshot};

pub(in crate::bounded_generics_v1) struct Project {
    pub(in crate::bounded_generics_v1) sources: SourceMap,
    pub(in crate::bounded_generics_v1) syntax: zryna_syntax::v5::VerifiedProjectSyntaxV5,
}

#[path = "body_fixture_declarations.rs"]
mod declarations;

pub(in crate::bounded_generics_v1) fn many_owners() -> Project {
    declarations::many_owners()
}

type Token = (String, u32, u32);

struct Reader {
    file: u32,
    tokens: Vec<Token>,
    cursor: usize,
    types: Vec<Value>,
    expressions: Vec<Value>,
    statements: Vec<Value>,
    blocks: Vec<Value>,
}

impl Reader {
    fn new(file: u32, source: &str) -> Self {
        let mut tokens = Vec::new();
        let mut index = 0;
        let bytes = source.as_bytes();
        while index < bytes.len() {
            if bytes[index].is_ascii_whitespace() {
                index += 1;
                continue;
            }
            let start = index;
            if bytes[index] == b'"' {
                index += 1;
                while bytes[index] != b'"' {
                    if bytes[index] == b'\\' {
                        index += 1;
                    }
                    index += 1;
                }
                index += 1;
            } else if bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_' {
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
            } else {
                let operator = ["===", "!==", "=>", "<=", ">="]
                    .into_iter()
                    .find(|operator| source[index..].starts_with(operator));
                index += operator.map_or(1, str::len);
            }
            tokens.push((
                source[start..index].into(),
                u32::try_from(start).expect("fixture offset"),
                u32::try_from(index).expect("fixture index"),
            ));
        }
        Self {
            file,
            tokens,
            cursor: 0,
            types: vec![],
            expressions: vec![],
            statements: vec![],
            blocks: vec![],
        }
    }

    fn peek(&self) -> &str {
        self.tokens.get(self.cursor).map_or("", |token| token.0.as_str())
    }
    fn start(&self) -> u32 {
        self.tokens[self.cursor].1
    }
    fn end(&self) -> u32 {
        self.tokens[self.cursor - 1].2
    }
    fn span(&self, start: u32, end: u32) -> Value {
        json!({"file":self.file,"start":start,"end":end})
    }
    fn take(&mut self) -> Token {
        let token = self.tokens[self.cursor].clone();
        self.cursor += 1;
        token
    }
    fn token(&mut self, expected: &str) -> Value {
        let (text, start, end) = self.take();
        assert_eq!(text, expected, "fixture token {start}");
        self.span(start, end)
    }
    fn identifier(&mut self) -> Value {
        let (text, start, end) = self.take();
        json!({"text":text,"span":self.span(start,end)})
    }
    fn emit(&mut self, start: u32, kind: Value) -> u32 {
        let index = u32::try_from(self.expressions.len()).expect("fixture arena");
        let mut expression = json!({"span":self.span(start,self.end())});
        expression["kind"] = kind;
        self.expressions.push(expression);
        index
    }
    fn expression(&mut self, minimum: u8) -> u32 {
        let start = self.start();
        let mut left = self.primary();
        loop {
            let (priority, tag) = match self.peek() {
                "===" => (1, "equal"),
                "!==" => (1, "not-equal"),
                "<" => (2, "less-than"),
                "<=" => (2, "less-equal"),
                ">" => (2, "greater-than"),
                ">=" => (2, "greater-equal"),
                "+" => (3, "addition"),
                "-" => (3, "subtraction"),
                "*" => (4, "multiplication"),
                _ => (0, ""),
            };
            if priority == 0 || priority < minimum {
                break;
            }
            let (_, begin, end) = self.take();
            let operator_span = self.span(begin, end);
            let right = self.expression(priority + 1);
            left = self.emit(
                start,
                json!({"kind":tag,"operator_span":operator_span,"lhs":left,"rhs":right}),
            );
        }
        left
    }
    fn primary(&mut self) -> u32 {
        let start = self.start();
        let value = if self.peek() == "-" {
            let operator_span = self.token("-");
            let operand = self.expression(5);
            self.emit(
                start,
                json!({"kind":"negation","operator_span":operator_span,"operand":operand}),
            )
        } else if self.peek() == "match" {
            self.match_expression()
        } else if matches!(self.peek(), "Vec" | "FixedArray") {
            let fixed = self.peek() == "FixedArray";
            let type_syntax = self.ty();
            let open_paren_span = self.token("(");
            let open_bracket_span = self.token("[");
            let elements = self.values("]");
            let close_bracket_span = self.token("]");
            let close_paren_span = self.token(")");
            self.emit(start,json!({"kind":if fixed {"fixed-array-construction"} else {"vec-construction"},"type_syntax":type_syntax,"open_paren_span":open_paren_span,"open_bracket_span":open_bracket_span,"elements":elements,"close_bracket_span":close_bracket_span,"close_paren_span":close_paren_span}))
        } else {
            let first = self.identifier();
            let text = first["text"].as_str().expect("original fixture invariant").to_owned();
            if text == "true" || text == "false" {
                self.emit(start, json!({"kind":"bool-literal","value":text=="true"}))
            } else if text.as_bytes()[0].is_ascii_digit() {
                self.emit(start, json!({"kind":"i32-literal","spelling":text}))
            } else if text.starts_with('"') {
                self.emit(start, json!({"kind":"string-literal","spelling":text}))
            } else {
                let enum_call = self.peek() == "."
                    && matches!(
                        self.tokens.get(self.cursor + 2).map(|token| token.0.as_str()),
                        Some("(" | "<")
                    );
                let member = if enum_call {
                    let dot = self.token(".");
                    Some((dot, self.identifier()))
                } else {
                    None
                };
                let type_arguments =
                    if self.application_call() { self.arguments() } else { Value::Null };
                if self.peek() == "(" {
                    let open_paren_span = self.token("(");
                    if self.peek() == "{" {
                        let open_brace_span = self.token("{");
                        let mut fields = vec![];
                        while self.peek() != "}" {
                            let begin = self.start();
                            let name = self.identifier();
                            let kind = if self.peek() == ":" {
                                let colon_span = self.token(":");
                                let value = self.expression(1);
                                json!({"kind":"explicit","name":name,"colon_span":colon_span,"value":value})
                            } else {
                                let value =
                                    self.emit(begin, json!({"kind":"reference","name":name}));
                                json!({"kind":"shorthand","name":name,"value":value})
                            };
                            fields.push(json!({"span":self.span(begin,self.end()),"kind":kind}));
                            if self.peek() != "," {
                                break;
                            }
                            self.token(",");
                        }
                        let close_brace_span = self.token("}");
                        let close_paren_span = self.token(")");
                        self.emit(start,json!({"kind":"struct-construction","type_name":first,"type_arguments":type_arguments,"open_paren_span":open_paren_span,"open_brace_span":open_brace_span,"fields":fields,"close_brace_span":close_brace_span,"close_paren_span":close_paren_span}))
                    } else {
                        let values = self.values(")");
                        let close_paren_span = self.token(")");
                        let kind = if let Some((dot_span, variant)) = member {
                            assert!(values.len() <= 1);
                            json!({"kind":"enum-construction","type_name":first,"dot_span":dot_span,"variant":variant,"type_arguments":type_arguments,"open_paren_span":open_paren_span,"payload":values.first(),"close_paren_span":close_paren_span})
                        } else if matches!(
                            text.as_str(),
                            "clone" | "shared" | "downgrade" | "borrow" | "borrowMut" | "push"
                        ) {
                            assert!(type_arguments.is_null());
                            if text == "push" {
                                assert_eq!(values.len(), 2);
                                json!({"kind":"vec-push","keyword_span":first["span"],"open_paren_span":open_paren_span,"vector":values[0],"comma_span":self.separator(values[0],values[1]),"value":values[1],"close_paren_span":close_paren_span})
                            } else {
                                assert_eq!(values.len(), 1);
                                json!({"kind":if text=="borrowMut" {"borrow-mut"} else {text.as_str()},"keyword_span":first["span"],"open_paren_span":open_paren_span,"value":values[0],"close_paren_span":close_paren_span})
                            }
                        } else {
                            json!({"kind":"call","callee":first,"type_arguments":type_arguments,"open_paren_span":open_paren_span,"arguments":values,"close_paren_span":close_paren_span})
                        };
                        self.emit(start, kind)
                    }
                } else {
                    assert!(type_arguments.is_null());
                    self.emit(start, json!({"kind":"reference","name":first}))
                }
            }
        };
        self.projection(start, value)
    }

    fn projection(&mut self, start: u32, mut value: u32) -> u32 {
        while matches!(self.peek(), "." | "[") {
            let kind = if self.peek() == "." {
                let dot_span = self.token(".");
                let field = self.identifier();
                json!({"kind":"field-access","base":value,"dot_span":dot_span,"field":field})
            } else {
                let open_bracket_span = self.token("[");
                let index = self.expression(1);
                let close_bracket_span = self.token("]");
                json!({"kind":"index","base":value,"open_bracket_span":open_bracket_span,"index":index,"close_bracket_span":close_bracket_span})
            };
            value = self.emit(start, kind);
        }
        value
    }
    fn separator(&self, left: u32, right: u32) -> Value {
        let end = self.expressions[left as usize]["span"]["end"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .expect("original fixture invariant");
        let start = self.expressions[right as usize]["span"]["start"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .expect("original fixture invariant");
        let token = self
            .tokens
            .iter()
            .find(|token| token.0 == "," && token.1 >= end && token.2 <= start)
            .expect("original fixture invariant");
        self.span(token.1, token.2)
    }
    fn application_call(&self) -> bool {
        if self.peek() != "<" {
            return false;
        }
        let mut depth = 0;
        for index in self.cursor..self.tokens.len() {
            match self.tokens[index].0.as_str() {
                "<" => depth += 1,
                ">" => {
                    depth -= 1;
                    if depth == 0 {
                        return self.tokens.get(index + 1).is_some_and(|token| token.0 == "(");
                    }
                }
                ";" | "=" | "{" | "}" => return false,
                _ => {}
            }
        }
        false
    }
    fn values(&mut self, close: &str) -> Vec<u32> {
        let mut values = vec![];
        while self.peek() != close {
            values.push(self.expression(1));
            if self.peek() != "," {
                break;
            }
            self.token(",");
        }
        values
    }
    fn match_expression(&mut self) -> u32 {
        let start = self.start();
        let keyword_span = self.token("match");
        let open_paren_span = self.token("(");
        let scrutinee = self.expression(1);
        self.token(",");
        let open_brace_span = self.token("{");
        let mut arms = vec![];
        while self.peek() != "}" {
            let (key, begin, end) = self.take();
            let (family, variant) =
                key.trim_matches('"').split_once('.').expect("original fixture invariant");
            let dot = begin + 1 + u32::try_from(family.len()).expect("fixture family length");
            let type_name = json!({"text":family,"span":self.span(begin+1,dot)});
            let variant = json!({"text":variant,"span":self.span(dot+1,end-1)});
            self.token(":");
            self.token("(");
            let binding = if self.peek() == ")" { Value::Null } else { self.identifier() };
            self.token(")");
            let arrow_span = self.token("=>");
            let value = self.expression(1);
            arms.push(json!({"span":self.span(begin,self.end()),"type_name":type_name,"dot_span":self.span(dot,dot+1),"variant":variant,"binding":binding,"arrow_span":arrow_span,"value":value}));
            if self.peek() != "," {
                break;
            }
            self.token(",");
        }
        let close_brace_span = self.token("}");
        let close_paren_span = self.token(")");
        self.emit(start,json!({"kind":"match","keyword_span":keyword_span,"open_paren_span":open_paren_span,"scrutinee":scrutinee,"close_paren_span":close_paren_span,"open_brace_span":open_brace_span,"arms":arms,"close_brace_span":close_brace_span}))
    }
    fn block(&mut self) -> u32 {
        let index = u32::try_from(self.blocks.len()).expect("fixture arena");
        self.blocks.push(Value::Null);
        let start = self.start();
        let open_brace_span = self.token("{");
        let mut statements = vec![];
        while self.peek() != "}" {
            statements.push(self.statement());
        }
        let close_brace_span = self.token("}");
        self.blocks[index as usize] = json!({"span":self.span(start,self.end()),"open_brace_span":open_brace_span,"statements":statements,"close_brace_span":close_brace_span});
        index
    }
    fn statement(&mut self) -> u32 {
        let index = u32::try_from(self.statements.len()).expect("fixture arena");
        self.statements.push(Value::Null);
        let start = self.start();
        let kind = match self.peek() {
            "const" | "let" => {
                let mutable = self.peek() == "let";
                let keyword_span = self.token(if mutable { "let" } else { "const" });
                let name = self.identifier();
                let type_syntax = if self.peek() == ":" {
                    self.token(":");
                    self.ty()
                } else {
                    let index = u32::try_from(self.types.len()).expect("fixture arena");
                    self.types.push(json!({"span":self.span(self.start(),self.start()),"kind":{"kind":"missing"}}));
                    index
                };
                let equals_span = self.token("=");
                let initializer = self.expression(1);
                let semicolon_span = self.token(";");
                json!({"kind":"local-declaration","keyword_span":keyword_span,"mutable":mutable,"name":name,"type_syntax":type_syntax,"equals_span":equals_span,"initializer":initializer,"semicolon_span":semicolon_span})
            }
            "return" => {
                let keyword_span = self.token("return");
                let value = self.expression(1);
                let semicolon_span = self.token(";");
                json!({"kind":"return","keyword_span":keyword_span,"value":value,"semicolon_span":semicolon_span})
            }
            "{" => {
                let block = self.block();
                json!({"kind":"block","block":block})
            }
            "if" | "while" => {
                let looped = self.peek() == "while";
                let keyword_span = self.token(if looped { "while" } else { "if" });
                let open_paren_span = self.token("(");
                let condition = self.expression(1);
                let close_paren_span = self.token(")");
                let block = self.block();
                if looped {
                    json!({"kind":"while","keyword_span":keyword_span,"open_paren_span":open_paren_span,"condition":condition,"close_paren_span":close_paren_span,"body_block":block})
                } else {
                    let else_clause = if self.peek() == "else" {
                        let keyword_span = self.token("else");
                        let block = self.block();
                        json!({"keyword_span":keyword_span,"block":block})
                    } else {
                        Value::Null
                    };
                    json!({"kind":"if","keyword_span":keyword_span,"open_paren_span":open_paren_span,"condition":condition,"close_paren_span":close_paren_span,"then_block":block,"else_clause":else_clause})
                }
            }
            "upgradeWeak" => {
                let keyword_span = self.token("upgradeWeak");
                self.token("(");
                let weak = self.expression(1);
                self.token(",");
                self.token("(");
                let binding = self.identifier();
                self.token(")");
                let as_span = self.token("=>");
                let success_block = self.block();
                self.token(",");
                self.token("(");
                self.token(")");
                let else_span = self.token("=>");
                let failure_block = self.block();
                self.token(")");
                self.token(";");
                json!({"kind":"weak-upgrade","keyword_span":keyword_span,"weak":weak,"as_span":as_span,"binding":binding,"success_block":success_block,"else_span":else_span,"failure_block":failure_block})
            }
            _ => {
                let expression = self.expression(1);
                if self.peek() == "=" {
                    let equals_span = self.token("=");
                    let value = self.expression(1);
                    let semicolon_span = self.token(";");
                    json!({"kind":"assignment","target":expression,"equals_span":equals_span,"value":value,"semicolon_span":semicolon_span})
                } else {
                    let semicolon_span = self.token(";");
                    json!({"kind":"expression-statement","expression":expression,"semicolon_span":semicolon_span})
                }
            }
        };
        self.statements[index as usize] = json!({"span":self.span(start,self.end()),"kind":kind});
        index
    }
}

pub(in crate::bounded_generics_v1) fn project(files: &[(&str, &str)]) -> Project {
    let mut files = files.to_vec();
    files.sort_unstable_by_key(|(path, _)| *path);
    let sources = SourceMap::build(
        files
            .iter()
            .map(|(path, text)| SourceFileInput { path: (*path).into(), text: (*text).into() })
            .collect(),
    )
    .expect("original fixture invariant");
    // Consume one unit's temporary JSON before constructing the next. The large owner fixture
    // must retain its exact source counts without keeping a second whole-project syntax tree.
    let raw = RawProjectSyntaxSnapshot {
        schema_version: 5,
        diagnostics: vec![],
        files: files
            .iter()
            .enumerate()
            .map(|(id, (path, text))| {
                serde_json::from_value(
                    Reader::new(u32::try_from(id).expect("fixture file"), text).unit(path),
                )
                .expect("original fixture invariant")
            })
            .collect(),
    };
    let syntax =
        verify_snapshot(raw, &sources).expect("independent complete original v5 verification");
    Project { sources, syntax }
}
