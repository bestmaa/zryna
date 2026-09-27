//! Bounded token-preserving layout for semantically admitted single-file M2 syntax.

#[derive(Clone, Copy, Eq, PartialEq)]
enum Kind {
    Word,
    Symbol,
    LineComment,
    BlockComment,
}

#[derive(Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    kind: Kind,
    separated_before: bool,
}

pub(super) fn format(source: &str) -> Option<String> {
    format_bounded(source, usize::MAX).ok()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LayoutError {
    Invalid,
    Limit,
}

pub(super) fn format_bounded(source: &str, maximum: usize) -> Result<String, LayoutError> {
    let tokens = tokenize(source).ok_or(LayoutError::Invalid)?;
    let mut output = String::new();
    let mut depth = 0_u8;
    let mut previous: Option<Token<'_>> = None;
    let mut previous_significant = None;
    let mut previous_unary = false;
    for token in tokens {
        let unary = token.text == "-"
            && previous_significant.is_none_or(|text| {
                matches!(
                    text,
                    "(" | ","
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
                )
            });
        if token.text == "}" {
            depth = depth.checked_sub(1).ok_or(LayoutError::Invalid)?;
            newline(&mut output);
        }
        if token.text == "else"
            && previous.is_some_and(|last| last.text == "}")
            && output.ends_with('\n')
        {
            output.pop();
        }
        if !output.is_empty()
            && !output.ends_with('\n')
            && needs_space(previous, token, unary, previous_unary)
        {
            output.push(' ');
        }
        if output.ends_with('\n') {
            for _ in 0..depth {
                output.push_str("  ");
            }
        }
        output.push_str(token.text);
        match token.text {
            "{" => {
                depth = depth.checked_add(1).ok_or(LayoutError::Invalid)?;
                newline(&mut output);
            }
            "}" | ";" => newline(&mut output),
            _ if token.kind == Kind::LineComment => newline(&mut output),
            _ => {}
        }
        if !matches!(token.kind, Kind::LineComment | Kind::BlockComment) {
            previous_significant = Some(token.text);
        }
        previous_unary = unary;
        previous = Some(token);
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

fn newline(output: &mut String) {
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
}

fn needs_space(
    previous: Option<Token<'_>>,
    token: Token<'_>,
    unary: bool,
    previous_unary: bool,
) -> bool {
    let Some(previous) = previous else { return false };
    if token.text == "else" && previous.text == "}" {
        return true;
    }
    if matches!(token.kind, Kind::LineComment | Kind::BlockComment)
        || matches!(previous.kind, Kind::LineComment | Kind::BlockComment)
    {
        return true;
    }
    if matches!(token.text, ")" | "," | ":" | ";") || previous.text == "(" {
        return false;
    }
    if token.text == "(" {
        return matches!(previous.text, "if" | "while" | "return");
    }
    if unary {
        return previous.text != "(";
    }
    if previous_unary && previous.text == "-" {
        return token.separated_before
            && token.text.as_bytes().first().is_some_and(u8::is_ascii_digit);
    }
    true
}

fn tokenize(mut source: &str) -> Option<Vec<Token<'_>>> {
    let mut result = Vec::new();
    while !source.is_empty() {
        let trimmed = source.trim_start_matches([' ', '\t', '\r', '\n']);
        let separated_before = trimmed.len() != source.len();
        source = trimmed;
        if source.is_empty() {
            break;
        }
        let (end, kind) = if source.starts_with("//") {
            (source.find(['\r', '\n']).unwrap_or(source.len()), Kind::LineComment)
        } else if source.starts_with("/*") {
            (source.find("*/")? + 2, Kind::BlockComment)
        } else if source.starts_with("===") || source.starts_with("!==") {
            (3, Kind::Symbol)
        } else if source.starts_with("<=") || source.starts_with(">=") {
            (2, Kind::Symbol)
        } else if source
            .starts_with(['(', ')', '{', '}', ',', ':', ';', '+', '-', '*', '<', '>', '='])
        {
            (1, Kind::Symbol)
        } else {
            let end = source
                .find(|c: char| c.is_whitespace() || "(){} ,:;+-*<>=!/[]?.\\\"'`".contains(c))
                .unwrap_or(source.len());
            if end == 0 {
                return None;
            }
            (end, Kind::Word)
        };
        result.push(Token { text: &source[..end], kind, separated_before });
        source = &source[end..];
    }
    Some(result)
}
