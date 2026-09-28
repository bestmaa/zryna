//! Token-preserving layout for the admitted protocol-v4 source grammar.

use zryna_frontend::native_lexer::{self, Lexeme, TokenKind, TriviaKind};
use zryna_source::{SourceFileInput, SourceMap};

use super::LayoutError;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Kind {
    Token(TokenKind),
    LineComment,
    BlockComment,
}

#[derive(Clone, Copy)]
struct Item<'a> {
    text: &'a str,
    kind: Kind,
    separated_before: bool,
}

pub(super) fn format_bounded(source: &str, maximum: usize) -> Result<String, LayoutError> {
    let items = items(source)?;
    let mut output = String::new();
    let mut depth = 0_u8;
    let mut previous: Option<Item<'_>> = None;
    let mut previous_significant = None;
    let mut previous_unary = false;
    for item in items {
        let unary = item.text == "-"
            && previous_significant.is_none_or(|text| {
                matches!(
                    text,
                    "(" | "["
                        | ","
                        | "="
                        | "+"
                        | "-"
                        | "*"
                        | "==="
                        | "!=="
                        | "<"
                        | "<="
                        | ">"
                        | ">="
                        | "return"
                        | "=>"
                )
            });
        if item.text == "}" {
            depth = depth.checked_sub(1).ok_or(LayoutError::Invalid)?;
            newline(&mut output);
        }
        if item.text == "else"
            && previous.is_some_and(|last| last.text == "}")
            && output.ends_with('\n')
        {
            output.pop();
        }
        if !output.is_empty()
            && !output.ends_with('\n')
            && needs_space(previous, item, unary, previous_unary)
        {
            output.push(' ');
        }
        if output.ends_with('\n') {
            for _ in 0..depth {
                output.push_str("  ");
            }
        }
        output.push_str(item.text);
        match item.text {
            "{" => {
                depth = depth.checked_add(1).ok_or(LayoutError::Invalid)?;
                newline(&mut output);
            }
            "}" | ";" => newline(&mut output),
            _ if item.kind == Kind::LineComment => newline(&mut output),
            _ => {}
        }
        if !matches!(item.kind, Kind::LineComment | Kind::BlockComment) {
            previous_significant = Some(item.text);
        }
        previous_unary = unary;
        previous = Some(item);
        if output.len() > maximum {
            return Err(LayoutError::Limit);
        }
    }
    if depth != 0 {
        return Err(LayoutError::Invalid);
    }
    newline(&mut output);
    if output.len() > maximum {
        return Err(LayoutError::Limit);
    }
    Ok(output)
}

fn items(source: &str) -> Result<Vec<Item<'_>>, LayoutError> {
    let sources = SourceMap::build(vec![SourceFileInput {
        path: "format.zry".to_owned(),
        text: source.to_owned(),
    }])
    .map_err(|_| LayoutError::Invalid)?;
    let lexed = native_lexer::lex(&sources).map_err(|_| LayoutError::Invalid)?;
    if !lexed.diagnostics().is_empty() {
        return Err(LayoutError::Invalid);
    }
    let file = lexed.files().first().ok_or(LayoutError::Invalid)?;
    let mut result = Vec::new();
    let mut separated_before = false;
    for lexeme in file.lexemes() {
        let span = lexeme.span();
        let text =
            source.get(span.start() as usize..span.end() as usize).ok_or(LayoutError::Invalid)?;
        let kind = match lexeme {
            Lexeme::Token(token) => Kind::Token(token.kind()),
            Lexeme::Trivia(trivia) => match trivia.kind() {
                TriviaKind::Whitespace => {
                    separated_before = true;
                    continue;
                }
                TriviaKind::LineComment => Kind::LineComment,
                TriviaKind::BlockComment => Kind::BlockComment,
            },
        };
        result.push(Item { text, kind, separated_before });
        separated_before = false;
    }
    Ok(result)
}

fn newline(output: &mut String) {
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
}

fn needs_space(
    previous: Option<Item<'_>>,
    item: Item<'_>,
    unary: bool,
    previous_unary: bool,
) -> bool {
    let Some(previous) = previous else { return false };
    if item.text == "else" && previous.text == "}" {
        return true;
    }
    if matches!(item.kind, Kind::LineComment | Kind::BlockComment)
        || matches!(previous.kind, Kind::LineComment | Kind::BlockComment)
    {
        return true;
    }
    if matches!(item.text, ")" | "]" | "," | ":" | ";" | ".")
        || matches!(previous.text, "(" | "[" | ".")
    {
        return false;
    }
    if item.text == "(" {
        return matches!(previous.text, "if" | "while" | "return" | "match" | "upgradeWeak");
    }
    if item.text == "[" {
        return false;
    }
    if matches!(item.text, "<" | ">") && matches!(previous.kind, Kind::Token(TokenKind::Identifier))
    {
        return false;
    }
    if matches!(previous.text, "<" | ">") && matches!(item.kind, Kind::Token(TokenKind::Identifier))
    {
        return false;
    }
    if unary {
        return previous.text != "(";
    }
    if previous_unary && previous.text == "-" {
        return item.separated_before
            && item.text.as_bytes().first().is_some_and(u8::is_ascii_digit);
    }
    true
}
