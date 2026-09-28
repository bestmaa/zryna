//! Bounded postorder type syntax for protocol v4.

use zryna_source::UntrustedSpan;
use zryna_syntax::v4 as syntax;

use crate::native_lexer::{Token, TokenKind};

use super::{FileParser, ParseError, raw, resource, unsupported};

struct TypeFrame {
    keyword: Token,
    less: Token,
    form: TypeForm,
}

enum TypeForm {
    Vec,
    Shared,
    Weak,
    Borrow,
    BorrowMut,
    FixedArray,
}

impl FileParser<'_> {
    fn push_type(&mut self, value: syntax::RawTypeSyntax) -> Result<u32, ParseError> {
        if self.types.len() >= syntax::MAX_TYPE_NODES_PER_MODULE
            || self.previous_types + self.types.len() >= syntax::MAX_TYPE_NODES_PER_PROJECT
        {
            return Err(resource("type-syntax inventory exceeds protocol-v4 limit"));
        }
        let id = u32::try_from(self.types.len()).expect("bounded type inventory");
        self.types.push(value);
        Ok(id)
    }

    fn finish_type_frame(&mut self, frame: &TypeFrame, child: u32) -> Result<u32, ParseError> {
        let kind = if matches!(frame.form, TypeForm::FixedArray) {
            let comma = self.take(TokenKind::Comma)?;
            let length_token = self.take(TokenKind::DecimalInteger)?;
            let spelling = self.spelling(length_token).to_owned();
            if spelling.len() > 10
                || (spelling != "0" && spelling.starts_with('0'))
                || !spelling
                    .parse::<u32>()
                    .is_ok_and(|length| length <= syntax::MAX_FIXED_ARRAY_LENGTH)
            {
                return Err(resource("fixed-array length exceeds protocol-v4 limit"));
            }
            let greater = self.take(TokenKind::GreaterThan)?;
            return self.push_type(syntax::RawTypeSyntax {
                span: UntrustedSpan {
                    file: self.file,
                    start: frame.keyword.span().start(),
                    end: greater.span().end(),
                },
                kind: syntax::RawTypeSyntaxKind::FixedArray {
                    keyword_span: raw(frame.keyword),
                    less_than_span: raw(frame.less),
                    element: child,
                    comma_span: raw(comma),
                    length_span: raw(length_token),
                    length_spelling: spelling.clone(),
                    length: spelling.parse().expect("bounded canonical length"),
                    greater_than_span: raw(greater),
                },
            });
        } else {
            let greater = self.take(TokenKind::GreaterThan)?;
            let keyword_span = raw(frame.keyword);
            let less_than_span = raw(frame.less);
            let greater_than_span = raw(greater);
            let kind = match frame.form {
                TypeForm::Vec => syntax::RawTypeSyntaxKind::Vec {
                    keyword_span,
                    less_than_span,
                    argument: child,
                    greater_than_span,
                },
                TypeForm::Shared => syntax::RawTypeSyntaxKind::Shared {
                    keyword_span,
                    less_than_span,
                    argument: child,
                    greater_than_span,
                },
                TypeForm::Weak => syntax::RawTypeSyntaxKind::Weak {
                    keyword_span,
                    less_than_span,
                    argument: child,
                    greater_than_span,
                },
                TypeForm::Borrow => syntax::RawTypeSyntaxKind::Borrow {
                    keyword_span,
                    less_than_span,
                    argument: child,
                    greater_than_span,
                },
                TypeForm::BorrowMut => syntax::RawTypeSyntaxKind::BorrowMut {
                    keyword_span,
                    less_than_span,
                    argument: child,
                    greater_than_span,
                },
                TypeForm::FixedArray => unreachable!("fixed array handled above"),
            };
            (kind, greater)
        };
        self.push_type(syntax::RawTypeSyntax {
            span: UntrustedSpan {
                file: self.file,
                start: frame.keyword.span().start(),
                end: kind.1.span().end(),
            },
            kind: kind.0,
        })
    }

    pub(super) fn type_syntax(&mut self) -> Result<u32, ParseError> {
        let mut frames = Vec::new();
        while let (Some(token), Some(next)) = (self.current(), self.next()) {
            if token.kind() != TokenKind::Identifier || next.kind() != TokenKind::LessThan {
                break;
            }
            let form = match self.spelling(token) {
                "Vec" => TypeForm::Vec,
                "Shared" => TypeForm::Shared,
                "Weak" => TypeForm::Weak,
                "Borrow" => TypeForm::Borrow,
                "BorrowMut" => TypeForm::BorrowMut,
                "FixedArray" => TypeForm::FixedArray,
                _ => return Err(unsupported(Some(token), "unsupported type constructor")),
            };
            if frames.len() + 1 >= syntax::MAX_NESTING_DEPTH as usize {
                return Err(resource("type syntax exceeds the nesting limit"));
            }
            self.position += 1;
            let less = self.take(TokenKind::LessThan)?;
            frames.push(TypeFrame { keyword: token, less, form });
        }
        if let Some(token) = self.current()
            && matches!(
                self.spelling(token),
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
            )
        {
            return Err(unsupported(Some(token), "unsupported primitive type annotation"));
        }
        let name = self.identifier()?;
        let mut id = self.push_type(syntax::RawTypeSyntax {
            span: name.span,
            kind: if name.text == "String" {
                syntax::RawTypeSyntaxKind::String { keyword_span: name.span }
            } else {
                syntax::RawTypeSyntaxKind::Named { name }
            },
        })?;
        while let Some(frame) = frames.pop() {
            id = self.finish_type_frame(&frame, id)?;
        }
        Ok(id)
    }

    pub(super) fn optional_type(&mut self, insertion: u32) -> Result<u32, ParseError> {
        if self.maybe(TokenKind::Colon).is_some() {
            self.type_syntax()
        } else {
            self.push_type(syntax::RawTypeSyntax {
                span: UntrustedSpan { file: self.file, start: insertion, end: insertion },
                kind: syntax::RawTypeSyntaxKind::Missing,
            })
        }
    }
}
