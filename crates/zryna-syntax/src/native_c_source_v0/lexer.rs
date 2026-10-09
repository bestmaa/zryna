use super::{MAX_TOKENS, SourceAuthError, error, limit, raw::Range};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Name,
    Integer,
    Key,
    Punctuation,
    End,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Token<'a> {
    pub kind: Kind,
    pub text: &'a str,
    pub range: Range,
}

fn name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

pub(super) fn lex(text: &str) -> Result<Vec<Token<'_>>, SourceAuthError> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let start = cursor;
        if b" \t\r\n".contains(&bytes[cursor]) {
            cursor += 1;
            continue;
        }
        if bytes[cursor..].starts_with(b"//") {
            cursor += 2;
            while cursor < bytes.len() && bytes[cursor] != b'\n' {
                cursor += 1;
            }
            continue;
        }
        if bytes[cursor..].starts_with(b"/*") {
            cursor += 2;
            while cursor < bytes.len() && !bytes[cursor..].starts_with(b"*/") {
                cursor += 1;
            }
            if cursor == bytes.len() {
                return Err(error("unterminated-comment", range(start, cursor)?));
            }
            cursor += 2;
            continue;
        }
        let kind;
        if name_start(bytes[cursor]) {
            kind = Kind::Name;
            cursor += 1;
            while cursor < bytes.len()
                && (name_start(bytes[cursor]) || bytes[cursor].is_ascii_digit())
            {
                cursor += 1;
            }
        } else if bytes[cursor].is_ascii_digit() {
            kind = Kind::Integer;
            cursor += 1;
            while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                cursor += 1;
            }
        } else if bytes[cursor] == b'"' {
            kind = Kind::Key;
            cursor += 1;
            while cursor < bytes.len() && bytes[cursor] != b'"' {
                if !matches!(bytes[cursor], 32..=126) || bytes[cursor] == b'\\' {
                    return Err(error("escaped-or-nonascii-key", range(start, cursor + 1)?));
                }
                cursor += 1;
            }
            if cursor == bytes.len() {
                return Err(error("unterminated-key", range(start, cursor)?));
            }
            cursor += 1;
        } else if bytes[cursor..].starts_with(b"!==") {
            kind = Kind::Punctuation;
            cursor += 3;
        } else if b"(){}:;,.<>+-=".contains(&bytes[cursor]) {
            kind = Kind::Punctuation;
            cursor += 1;
        } else {
            return Err(error("unsupported-token", range(start, start + 1)?));
        }
        let range = range(start, cursor)?;
        if (kind == Kind::Name && cursor - start > 128)
            || (kind == Kind::Key && cursor - start > 259)
            || (kind == Kind::Integer && cursor - start > 10)
        {
            return Err(limit("token-bytes", range));
        }
        // Every emitted token contains ASCII only; arbitrary UTF-8 is admitted only in comments.
        let token = text.get(start..cursor).ok_or_else(|| error("token-boundary", range))?;
        if tokens.len() == MAX_TOKENS {
            return Err(limit("tokens", range));
        }
        tokens.push(Token { kind, text: token, range });
    }
    let end = u32::try_from(bytes.len())
        .map_err(|_| limit("source-bytes", Range { start: 0, end: 0 }))?;
    tokens.push(Token { kind: Kind::End, text: "", range: Range { start: end, end } });
    Ok(tokens)
}

fn range(start: usize, end: usize) -> Result<Range, SourceAuthError> {
    let overflow = || limit("source-bytes", Range { start: 0, end: 0 });
    Ok(Range {
        start: u32::try_from(start).map_err(|_| overflow())?,
        end: u32::try_from(end).map_err(|_| overflow())?,
    })
}
