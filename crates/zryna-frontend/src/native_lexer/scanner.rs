use zryna_diagnostics::Diagnostic;
use zryna_source::{FileId, SourceMap, Span};

use super::{
    Keyword, LexError, Lexeme, MAX_LEXICAL_DIAGNOSTICS, MAX_TOKENS_PER_FILE, MAX_TRIVIA_PER_FILE,
    Token, TokenKind, Trivia, TriviaKind, resource, resource_at,
};

pub(super) fn scan_file(
    sources: &SourceMap,
    file: FileId,
    text: &str,
    remaining_project_lexemes: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<Lexeme>, LexError> {
    let bytes = text.as_bytes();
    let mut lexemes = Vec::new();
    let mut offset = 0;
    let mut tokens = 0;
    let mut trivia = 0;
    while offset < bytes.len() {
        let start = offset;
        let (kind, end, problem) = if let Some(width) = whitespace_width(&bytes[start..]) {
            offset += width;
            while offset < bytes.len() {
                let Some(width) = whitespace_width(&bytes[offset..]) else {
                    break;
                };
                offset += width;
            }
            (ItemKind::Trivia(TriviaKind::Whitespace), offset, None)
        } else if bytes[start..].starts_with(b"//") {
            offset += 2;
            while offset < bytes.len() && line_terminator_width(&bytes[offset..]).is_none() {
                offset += utf8_width(bytes[offset]);
            }
            (ItemKind::Trivia(TriviaKind::LineComment), offset, None)
        } else if bytes[start..].starts_with(b"/*") {
            offset += 2;
            if let Some(relative_end) = text[offset..].find("*/") {
                offset += relative_end + 2;
                (ItemKind::Trivia(TriviaKind::BlockComment), offset, None)
            } else {
                offset = bytes.len();
                (
                    ItemKind::Trivia(TriviaKind::BlockComment),
                    offset,
                    Some("unterminated block comment"),
                )
            }
        } else if matches!(bytes[start], b'\'' | b'"') {
            scan_string(bytes, start)
        } else if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' {
            offset += 1;
            while offset < bytes.len()
                && (bytes[offset].is_ascii_alphanumeric() || bytes[offset] == b'_')
            {
                offset += 1;
            }
            let spelling = &text[start..offset];
            if spelling.len() > 128 || matches!(spelling, "constructor" | "prototype" | "__proto__")
            {
                (
                    ItemKind::Token(TokenKind::Invalid),
                    offset,
                    Some("identifier spelling is forbidden or exceeds 128 bytes"),
                )
            } else {
                (ItemKind::Token(keyword(spelling)), offset, None)
            }
        } else if bytes[start].is_ascii_digit() {
            offset += 1;
            while offset < bytes.len() && bytes[offset].is_ascii_digit() {
                offset += 1;
            }
            (ItemKind::Token(TokenKind::DecimalInteger), offset, None)
        } else if let Some((kind, width)) = punctuation(&bytes[start..]) {
            offset += width;
            (ItemKind::Token(kind), offset, None)
        } else {
            offset += utf8_width(bytes[start]);
            (ItemKind::Token(TokenKind::Invalid), offset, Some("unrecognized source character"))
        };
        let span = make_span(sources, file, start, end)?;
        push_item(kind, span, remaining_project_lexemes, &mut lexemes, &mut tokens, &mut trivia)?;
        if let Some(message) = problem {
            malformed(diagnostics, span, message)?;
        }
        offset = end;
    }
    Ok(lexemes)
}

fn push_item(
    kind: ItemKind,
    span: Span,
    remaining_project_lexemes: usize,
    lexemes: &mut Vec<Lexeme>,
    tokens: &mut usize,
    trivia: &mut usize,
) -> Result<(), LexError> {
    let lexeme = match kind {
        ItemKind::Token(kind) => {
            *tokens += 1;
            if *tokens > MAX_TOKENS_PER_FILE {
                return Err(resource_at(span, "source file token inventory exceeds its limit"));
            }
            Lexeme::Token(Token { kind, span })
        }
        ItemKind::Trivia(kind) => {
            *trivia += 1;
            if *trivia > MAX_TRIVIA_PER_FILE {
                return Err(resource_at(span, "source file trivia inventory exceeds its limit"));
            }
            Lexeme::Trivia(Trivia { kind, span })
        }
    };
    if lexemes.len() == remaining_project_lexemes {
        return Err(resource_at(span, "project lexical inventory exceeds its limit"));
    }
    lexemes.push(lexeme);
    Ok(())
}

#[derive(Clone, Copy)]
enum ItemKind {
    Token(TokenKind),
    Trivia(TriviaKind),
}

fn scan_string(bytes: &[u8], start: usize) -> (ItemKind, usize, Option<&'static str>) {
    let quote = bytes[start];
    let mut offset = start + 1;
    let mut escaped = false;
    while offset < bytes.len() {
        if line_terminator_width(&bytes[offset..]).is_some() {
            return (
                ItemKind::Token(TokenKind::Invalid),
                offset,
                Some("unterminated string literal"),
            );
        }
        match bytes[offset] {
            byte if byte == quote => {
                offset += 1;
                return if escaped {
                    (
                        ItemKind::Token(TokenKind::Invalid),
                        offset,
                        Some("string escapes are unsupported"),
                    )
                } else {
                    (ItemKind::Token(TokenKind::StringLiteral), offset, None)
                };
            }
            b'\\' => {
                escaped = true;
                offset += 1;
                if offset < bytes.len() && line_terminator_width(&bytes[offset..]).is_none() {
                    offset += utf8_width(bytes[offset]);
                }
            }
            byte => offset += utf8_width(byte),
        }
    }
    (ItemKind::Token(TokenKind::Invalid), offset, Some("unterminated string literal"))
}

fn punctuation(bytes: &[u8]) -> Option<(TokenKind, usize)> {
    for (spelling, kind) in [
        (&b"!=="[..], TokenKind::StrictNotEqual),
        (&b"==="[..], TokenKind::StrictEqual),
        (&b"=>"[..], TokenKind::FatArrow),
        (&b"<="[..], TokenKind::LessEqual),
        (&b">="[..], TokenKind::GreaterEqual),
    ] {
        if bytes.starts_with(spelling) {
            return Some((kind, spelling.len()));
        }
    }
    Some((
        match bytes[0] {
            b'{' => TokenKind::OpenBrace,
            b'}' => TokenKind::CloseBrace,
            b'[' => TokenKind::OpenBracket,
            b']' => TokenKind::CloseBracket,
            b'(' => TokenKind::OpenParen,
            b')' => TokenKind::CloseParen,
            b':' => TokenKind::Colon,
            b';' => TokenKind::Semicolon,
            b',' => TokenKind::Comma,
            b'.' => TokenKind::Dot,
            b'<' => TokenKind::LessThan,
            b'>' => TokenKind::GreaterThan,
            b'=' => TokenKind::Equals,
            b'+' => TokenKind::Plus,
            b'-' => TokenKind::Minus,
            b'*' => TokenKind::Asterisk,
            _ => return None,
        },
        1,
    ))
}

fn keyword(spelling: &str) -> TokenKind {
    let keyword = match spelling {
        "as" => Keyword::As,
        "const" => Keyword::Const,
        "else" => Keyword::Else,
        "export" => Keyword::Export,
        "extends" => Keyword::Extends,
        "false" => Keyword::False,
        "from" => Keyword::From,
        "function" => Keyword::Function,
        "if" => Keyword::If,
        "import" => Keyword::Import,
        "interface" => Keyword::Interface,
        "let" => Keyword::Let,
        "return" => Keyword::Return,
        "true" => Keyword::True,
        "while" => Keyword::While,
        _ => return TokenKind::Identifier,
    };
    TokenKind::Keyword(keyword)
}

fn utf8_width(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first < 0xe0 {
        2
    } else if first < 0xf0 {
        3
    } else {
        4
    }
}

fn whitespace_width(bytes: &[u8]) -> Option<usize> {
    if matches!(bytes[0], b' ' | b'\t' | b'\x0b' | b'\x0c' | b'\r' | b'\n') {
        return Some(1);
    }
    let width = utf8_width(bytes[0]);
    let character = std::str::from_utf8(bytes.get(..width)?).ok()?.chars().next()?;
    matches!(
        character,
        '\u{0085}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200b}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
    .then_some(width)
}

fn line_terminator_width(bytes: &[u8]) -> Option<usize> {
    if matches!(bytes[0], b'\r' | b'\n') {
        Some(1)
    } else if bytes.starts_with("\u{2028}".as_bytes()) || bytes.starts_with("\u{2029}".as_bytes()) {
        Some(3)
    } else {
        None
    }
}

fn make_span(
    sources: &SourceMap,
    file: FileId,
    start: usize,
    end: usize,
) -> Result<Span, LexError> {
    let start = u32::try_from(start).map_err(|_| resource("source offset overflow"))?;
    let end = u32::try_from(end).map_err(|_| resource("source offset overflow"))?;
    sources.span(file, start, end).map_err(|error| super::source_error(&error))
}

fn malformed(
    diagnostics: &mut Vec<Diagnostic>,
    span: Span,
    message: &'static str,
) -> Result<(), LexError> {
    if diagnostics.len() == MAX_LEXICAL_DIAGNOSTICS {
        return Err(resource_at(span, "lexical diagnostics exceed their limit"));
    }
    diagnostics.push(Diagnostic::error_at(
        "ZRYNA-F1501",
        span,
        message,
        "use the frozen protocol-v4 lexical spelling",
    ));
    Ok(())
}
