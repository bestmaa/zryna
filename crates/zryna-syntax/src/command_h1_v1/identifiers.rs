//! Bounded source-byte inventory independent of provider expression claims.

pub(super) fn inventory(source: &str) -> Result<Vec<(&str, u32, u32)>, &'static str> {
    let bytes = source.as_bytes();
    let mut cursor = 0;
    let mut result = Vec::new();
    let mut previous = None;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\'' | b'"' => {
                let quote = bytes[cursor];
                cursor += 1;
                loop {
                    let Some(&byte) = bytes.get(cursor) else {
                        return Err("unterminated command source string");
                    };
                    cursor += 1;
                    if byte == quote {
                        break;
                    }
                    if byte == b'\\' {
                        if cursor == bytes.len() {
                            return Err("unterminated command source escape");
                        }
                        cursor += 1;
                    }
                }
                previous = Some(quote);
            }
            b'/' if bytes.get(cursor + 1) == Some(&b'/') => {
                cursor += 2;
                while cursor < bytes.len()
                    && !matches!(bytes[cursor], b'\n' | b'\r')
                    && !matches!(
                        bytes.get(cursor..cursor + 3),
                        Some(b"\xe2\x80\xa8" | b"\xe2\x80\xa9")
                    )
                {
                    cursor += 1;
                }
            }
            b'/' if bytes.get(cursor + 1) == Some(&b'*') => {
                cursor += 2;
                while bytes.get(cursor..cursor + 2) != Some(b"*/") {
                    if cursor >= bytes.len() {
                        return Err("unterminated command source comment");
                    }
                    cursor += 1;
                }
                cursor += 2;
            }
            b'`' | b'\\' | b'$' => {
                return Err("command source cannot hide identifiers in templates or escapes");
            }
            byte if byte.is_ascii_alphanumeric() || byte == b'_' => {
                let start = cursor;
                cursor += 1;
                while bytes
                    .get(cursor)
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                {
                    cursor += 1;
                }
                let name = &source[start..cursor];
                if matches!(name, super::INTRINSIC | super::OUTCOME) {
                    if previous == Some(b'.') {
                        return Err("command reserved name cannot be selected as a property");
                    }
                    result.push((
                        name,
                        u32::try_from(start).map_err(|_| "command source exceeds its bound")?,
                        u32::try_from(cursor).map_err(|_| "command source exceeds its bound")?,
                    ));
                }
                previous = Some(b'a');
            }
            byte if byte >= 128 => {
                let character = source[cursor..].chars().next().ok_or("invalid command UTF-8")?;
                if !source_whitespace(character) {
                    return Err("command source requires ASCII identifiers");
                }
                cursor += character.len_utf8();
            }
            byte => {
                if !byte.is_ascii_whitespace() {
                    previous = Some(byte);
                }
                cursor += 1;
            }
        }
    }
    Ok(result)
}

pub(super) fn source_call(
    source: &str,
    start: u32,
    callee_end: u32,
) -> Result<(u32, u32, u32), &'static str> {
    let bytes = source.as_bytes();
    let start = usize::try_from(start).map_err(|_| "command source exceeds its bound")?;
    let callee_end = usize::try_from(callee_end).map_err(|_| "command source exceeds its bound")?;
    if source.get(start..callee_end) != Some(super::INTRINSIC)
        || source
            .get(..start)
            .and_then(|prefix| prefix.chars().next_back())
            .is_some_and(identifier_part)
        || source
            .get(callee_end..)
            .and_then(|tail| tail.chars().next())
            .is_some_and(identifier_part)
    {
        return Err("environment lookup requires an exact standalone identifier");
    }
    if source[..start].trim_end_matches(source_whitespace).ends_with('.') {
        return Err("environment lookup cannot be a property operation");
    }
    let mut cursor = callee_end;
    trivia(bytes, &mut cursor)?;
    if bytes.get(cursor) != Some(&b'(') {
        return Err("environment lookup must be an ordinary direct call");
    }
    cursor += 1;
    trivia(bytes, &mut cursor)?;
    let literal_start = cursor;
    let quote = *bytes.get(cursor).ok_or("environment lookup is missing its literal key")?;
    if !matches!(quote, b'\'' | b'"') {
        return Err("environment lookup requires a source literal");
    }
    cursor += 1;
    while bytes.get(cursor) != Some(&quote) {
        if cursor == bytes.len() || matches!(bytes[cursor], b'\\' | b'\n' | b'\r' | 0) {
            return Err("environment lookup requires an unescaped source literal");
        }
        cursor += 1;
    }
    cursor += 1;
    let literal_end = cursor;
    trivia(bytes, &mut cursor)?;
    if bytes.get(cursor) == Some(&b',') {
        cursor += 1;
        trivia(bytes, &mut cursor)?;
    }
    if bytes.get(cursor) != Some(&b')') {
        return Err("environment lookup admits exactly one literal argument");
    }
    Ok((
        u32::try_from(literal_start).map_err(|_| "command source exceeds its bound")?,
        u32::try_from(literal_end).map_err(|_| "command source exceeds its bound")?,
        u32::try_from(cursor + 1).map_err(|_| "command source exceeds its bound")?,
    ))
}

pub(super) fn identifier_part(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(character, '_' | '$')
        || (!character.is_ascii() && !source_whitespace(character))
}

// Exact WhiteSpaceLike set of the locked TypeScript 6 source scanner, including its
// historical U+0085/U+200B extensions. Rust Unicode whitespace is a different set.
fn source_whitespace(character: char) -> bool {
    matches!(
        character,
        '\t' | '\n'
            | '\u{000b}'
            | '\u{000c}'
            | '\r'
            | ' '
            | '\u{0085}'
            | '\u{00a0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200b}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

pub(super) fn trivia(bytes: &[u8], cursor: &mut usize) -> Result<(), &'static str> {
    loop {
        let tail = std::str::from_utf8(&bytes[*cursor..]).map_err(|_| "invalid command UTF-8")?;
        let trimmed = tail.trim_start_matches(source_whitespace);
        *cursor += tail.len() - trimmed.len();
        if bytes.get(*cursor..*cursor + 2) == Some(b"/*") {
            *cursor += 2;
            while bytes.get(*cursor..*cursor + 2) != Some(b"*/") {
                if *cursor >= bytes.len() {
                    return Err("unterminated command source comment");
                }
                *cursor += 1;
            }
            *cursor += 2;
        } else if bytes.get(*cursor..*cursor + 2) == Some(b"//") {
            let tail =
                std::str::from_utf8(&bytes[*cursor..]).map_err(|_| "invalid command UTF-8")?;
            *cursor += tail.find(['\n', '\r', '\u{2028}', '\u{2029}']).unwrap_or(tail.len());
        } else {
            return Ok(());
        }
    }
}
