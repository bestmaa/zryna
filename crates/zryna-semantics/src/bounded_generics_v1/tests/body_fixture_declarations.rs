//! Source declaration and type occurrence serialization for the independent test reader.

use super::Reader;
use std::fmt::Write;

pub(super) fn many_owners() -> super::Project {
    let paths = ["main.zry", "one.zry", "two.zry", "three.zry"];
    let mut files = Vec::new();
    for (module, path) in paths.into_iter().enumerate() {
        let mut source = String::new();
        if module == 0 {
            for name in ["one", "two", "three"] {
                writeln!(source, "import {{f0 as {name}}} from \"./{name}.zry\";")
                    .expect("fixture source");
            }
        }
        for index in 0..4096 {
            let extra = if module == 0 && index < 9 { "extra:i32;" } else { "" };
            writeln!(source, "interface D{index}<T extends ZrynaValue,E extends ZrynaValue> extends ZrynaStruct {{value:T;{extra}}}").expect("fixture source");
        }
        for index in 0..4095 {
            let export = if index == 0 { "export " } else { "" };
            writeln!(source, "{export}function f{index}<T extends ZrynaValue,E extends ZrynaValue>(value:T):T {{return value;}}").expect("fixture source");
        }
        files.push((path, source));
    }
    super::project(&files.iter().map(|(path, text)| (*path, text.as_str())).collect::<Vec<_>>())
}
use serde_json::{Value, json};

impl Reader {
    pub(super) fn parameters(&mut self) -> Value {
        if self.peek() != "<" {
            return Value::Null;
        }
        let start = self.start();
        let less_than_span = self.token("<");
        let mut parameters = vec![];
        let mut comma_spans = vec![];
        loop {
            let begin = self.start();
            let name = self.identifier();
            let extends_span = self.token("extends");
            let bound = self.identifier();
            parameters.push(json!({"span":self.span(begin,self.end()),"name":name,"extends_span":extends_span,"bound":bound}));
            if self.peek() != "," {
                break;
            }
            comma_spans.push(self.token(","));
        }
        let greater_than_span = self.token(">");
        json!({"span":self.span(start,self.end()),"less_than_span":less_than_span,"parameters":parameters,"comma_spans":comma_spans,"greater_than_span":greater_than_span})
    }
    pub(super) fn arguments(&mut self) -> Value {
        if self.peek() != "<" {
            return Value::Null;
        }
        let start = self.start();
        let less_than_span = self.token("<");
        let mut arguments = vec![];
        let mut comma_spans = vec![];
        loop {
            arguments.push(self.ty());
            if self.peek() != "," {
                break;
            }
            comma_spans.push(self.token(","));
        }
        let greater_than_span = self.token(">");
        json!({"span":self.span(start,self.end()),"less_than_span":less_than_span,"arguments":arguments,"comma_spans":comma_spans,"greater_than_span":greater_than_span})
    }
    pub(super) fn ty(&mut self) -> u32 {
        let start = self.start();
        let name = self.identifier();
        let text = name["text"].as_str().expect("original fixture invariant");
        let kind = if matches!(
            text,
            "Vec" | "Shared" | "Weak" | "Borrow" | "BorrowMut" | "FixedArray"
        ) {
            let keyword_span = name["span"].clone();
            let less_than_span = self.token("<");
            let argument = self.ty();
            if text == "FixedArray" {
                let comma_span = self.token(",");
                let (spelling, begin, end) = self.take();
                let greater_than_span = self.token(">");
                json!({"kind":"fixed-array","keyword_span":keyword_span,"less_than_span":less_than_span,"element":argument,"comma_span":comma_span,"length_span":self.span(begin,end),"length":spelling.parse::<u32>().expect("original fixture invariant"),"length_spelling":spelling,"greater_than_span":greater_than_span})
            } else {
                let greater_than_span = self.token(">");
                let tag = match text {
                    "BorrowMut" => "borrow-mut",
                    "Shared" => "shared",
                    "Weak" => "weak",
                    "Borrow" => "borrow",
                    _ => "vec",
                };
                json!({"kind":tag,"keyword_span":keyword_span,"less_than_span":less_than_span,"argument":argument,"greater_than_span":greater_than_span})
            }
        } else if text == "String" {
            json!({"kind":"string","keyword_span":name["span"]})
        } else if self.peek() == "<" {
            let type_arguments = self.arguments();
            json!({"kind":"application","name":name,"type_arguments":type_arguments})
        } else {
            json!({"kind":"named","name":name})
        };
        let index = u32::try_from(self.types.len()).expect("fixture type count");
        self.types.push(json!({"span":self.span(start,self.end()),"kind":kind}));
        index
    }
    pub(super) fn unit(mut self, path: &str) -> Value {
        let mut functions = vec![];
        let mut data = vec![];
        let mut imports = vec![];
        while !self.peek().is_empty() {
            let start = self.start();
            let export_span =
                if self.peek() == "export" { self.token("export") } else { Value::Null };
            match self.peek() {
                "function" => {
                    let function_span = self.token("function");
                    let name = self.identifier();
                    let type_parameters = self.parameters();
                    self.token("(");
                    let mut parameters = vec![];
                    while self.peek() != ")" {
                        let begin = self.start();
                        let name = self.identifier();
                        self.token(":");
                        let type_syntax = self.ty();
                        parameters.push(json!({"span":self.span(begin,self.end()),"name":name,"type_syntax":type_syntax}));
                        if self.peek() != "," {
                            break;
                        }
                        self.token(",");
                    }
                    self.token(")");
                    self.token(":");
                    let result_type = self.ty();
                    let root_block = self.block();
                    let body = json!({"span":self.blocks[root_block as usize]["span"],"root_block":root_block,"blocks":self.blocks,"statements":self.statements,"expressions":self.expressions});
                    functions.push(json!({"span":self.span(start,self.end()),"export_span":export_span,"function_span":function_span,"name":name,"type_parameters":type_parameters,"parameters":parameters,"result_type":result_type,"body":body}));
                    self.blocks.clear();
                    self.statements.clear();
                    self.expressions.clear();
                }
                "interface" => {
                    let interface_span = self.token("interface");
                    let name = self.identifier();
                    let type_parameters = self.parameters();
                    let extends_span = self.token("extends");
                    let enumeration = self.peek() == "ZrynaEnum";
                    let marker_span =
                        self.token(if enumeration { "ZrynaEnum" } else { "ZrynaStruct" });
                    let open_brace_span = self.token("{");
                    let mut members = vec![];
                    while self.peek() != "}" {
                        let begin = self.start();
                        let name = self.identifier();
                        let colon_span = self.token(":");
                        let empty = enumeration && self.peek() == "ZrynaNone";
                        let none_span = if empty { self.token("ZrynaNone") } else { Value::Null };
                        let ty = if empty { None } else { Some(self.ty()) };
                        let semicolon_span = self.token(";");
                        members.push(if enumeration {json!({"span":self.span(begin,self.end()),"name":name,"colon_span":colon_span,"payload_type":ty,"none_span":none_span,"semicolon_span":semicolon_span})} else {json!({"span":self.span(begin,self.end()),"name":name,"colon_span":colon_span,"type_syntax":ty.expect("original fixture invariant"),"semicolon_span":semicolon_span})});
                    }
                    let close_brace_span = self.token("}");
                    let mut kind = json!({"kind":if enumeration {"enum"} else {"struct"},"interface_span":interface_span,"name":name,"extends_span":extends_span,"marker_span":marker_span,"open_brace_span":open_brace_span,"close_brace_span":close_brace_span});
                    kind[if enumeration { "variants" } else { "fields" }] = json!(members);
                    data.push(json!({"span":self.span(start,self.end()),"export_span":export_span,"type_parameters":type_parameters,"kind":kind}));
                }
                "import" => {
                    let import_span = self.token("import");
                    self.token("{");
                    let mut bindings = vec![];
                    while self.peek() != "}" {
                        let begin = self.start();
                        let imported = self.identifier();
                        let as_span =
                            if self.peek() == "as" { self.token("as") } else { Value::Null };
                        let local =
                            if as_span.is_null() { imported.clone() } else { self.identifier() };
                        bindings.push(json!({"span":self.span(begin,self.end()),"imported":imported,"local":local,"as_span":as_span}));
                        if self.peek() != "," {
                            break;
                        }
                        self.token(",");
                    }
                    self.token("}");
                    let from_span = self.token("from");
                    let (text, begin, end) = self.take();
                    let specifier = json!({"text":text.trim_matches('"'),"token_span":self.span(begin,end),"value_span":self.span(begin+1,end-1)});
                    let semicolon_span = self.token(";");
                    imports.push(json!({"span":self.span(start,self.end()),"import_span":import_span,"bindings":bindings,"from_span":from_span,"specifier":specifier,"semicolon_span":semicolon_span}));
                }
                other => panic!("fixture declaration {other}"),
            }
        }
        json!({"id":self.file,"path":path,"imports":imports,"type_syntax":self.types,"data_declarations":data,"functions":functions})
    }
}
