//! Lexical identifier roles follow the accepted v5 source grammar, not semantic resolution.

use super::{FileParser, ParseError, syntax, unsupported};
use crate::native_lexer::{Keyword, Token, TokenKind};

#[derive(Clone, Copy)]
pub(super) enum Role {
    Function,
    Data,
    Parameter,
    Value,
    Local,
    Runtime,
    Type,
    Assignment,
}

fn runtime_reserved(text: &str) -> bool {
    matches!(
        text,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
    )
}

fn strict_word(text: &str) -> bool {
    matches!(
        text,
        "implements"
            | "interface"
            | "let"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "static"
            | "yield"
    )
}

pub(super) fn external_module(tokens: &[Token]) -> bool {
    let mut depth = 0_u32;
    for token in tokens {
        match token.kind() {
            TokenKind::Keyword(Keyword::Import | Keyword::Export) if depth == 0 => return true,
            TokenKind::OpenParen | TokenKind::OpenBrace | TokenKind::OpenBracket => depth += 1,
            TokenKind::CloseParen | TokenKind::CloseBrace | TokenKind::CloseBracket => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    false
}

impl FileParser<'_> {
    pub(super) fn name(&mut self, role: Role) -> Result<syntax::RawIdentifierSyntax, ParseError> {
        let name = self.identifier()?;
        self.name_role(&name, role)?;
        Ok(name)
    }

    pub(super) fn name_role(
        &self,
        name: &syntax::RawIdentifierSyntax,
        role: Role,
    ) -> Result<(), ParseError> {
        let text = name.text.as_str();
        let invalid = if matches!(role, Role::Type) {
            self.external_module && (strict_word(text) || !self.in_function && text == "await")
        } else {
            runtime_reserved(text)
                || self.external_module && strict_word(text)
                || match role {
                    Role::Function => {
                        self.external_module && matches!(text, "await" | "eval" | "arguments")
                    }
                    Role::Data => self.external_module && text == "await",
                    Role::Parameter => self.external_module && !self.in_function && text == "await",
                    Role::Value | Role::Assignment => {
                        self.external_module && matches!(text, "eval" | "arguments")
                    }
                    Role::Local => {
                        text == "let"
                            || self.external_module && matches!(text, "eval" | "arguments")
                    }
                    Role::Runtime => false,
                    Role::Type => unreachable!("type role handled above"),
                }
        };
        if invalid {
            return Err(unsupported(
                self.tokens.get(self.position.saturating_sub(1)).copied(),
                "identifier is excluded in this v5 source role",
            ));
        }
        Ok(())
    }
}

pub(super) fn type_diversion(text: &str) -> bool {
    matches!(
        text,
        "any"
            | "unknown"
            | "never"
            | "number"
            | "string"
            | "boolean"
            | "symbol"
            | "bigint"
            | "undefined"
            | "object"
            | "true"
            | "false"
            | "null"
            | "void"
            | "this"
            | "typeof"
            | "import"
            | "new"
            | "keyof"
            | "unique"
            | "readonly"
            | "infer"
    )
}
