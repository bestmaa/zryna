//! Source-backed token and trivia consumption for v5 source claims.

use zryna_source::{FileId, SourceMap, Span, UntrustedSpan};

use super::DeclarationError;
use crate::v4::RawIdentifierSyntax;

pub(super) struct Cursor<'a> {
    sources: &'a SourceMap,
    file: FileId,
    text: &'a str,
    offset: usize,
    end: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(
        sources: &'a SourceMap,
        span: UntrustedSpan,
    ) -> Result<Self, DeclarationError> {
        let bound = sources.verify_span(span).map_err(|_| DeclarationError::malformed(None))?;
        let source =
            sources.source(bound.file()).ok_or_else(|| DeclarationError::malformed(None))?;
        Ok(Self {
            sources,
            file: bound.file(),
            text: source.text(),
            offset: span.start as usize,
            end: span.end as usize,
        })
    }

    pub(super) fn bound(&self, span: UntrustedSpan) -> Result<Span, DeclarationError> {
        let bound =
            self.sources.verify_span(span).map_err(|_| DeclarationError::malformed(None))?;
        if bound.file() != self.file || span.end as usize > self.end {
            return Err(DeclarationError::malformed(Some(bound)));
        }
        Ok(bound)
    }

    pub(super) fn token(
        &mut self,
        span: UntrustedSpan,
        token: &str,
    ) -> Result<(), DeclarationError> {
        let bound = self.bound(span)?;
        if (span.start as usize) < self.offset
            || self.text.get(span.start as usize..span.end as usize) != Some(token)
        {
            return Err(DeclarationError::malformed(Some(bound)));
        }
        if !trivia(&self.text[self.offset..span.start as usize]) {
            return Err(DeclarationError::malformed(Some(bound)));
        }
        let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
        if token.bytes().next().is_some_and(word) && token.bytes().last().is_some_and(word) {
            let start = span.start as usize;
            let end = span.end as usize;
            if (start > 0 && word(self.text.as_bytes()[start - 1]))
                || self.text.as_bytes().get(end).copied().is_some_and(word)
            {
                return Err(DeclarationError::malformed(Some(bound)));
            }
        }
        self.offset = span.end as usize;
        Ok(())
    }

    pub(super) fn identifier(
        &mut self,
        name: &RawIdentifierSyntax,
    ) -> Result<(), DeclarationError> {
        self.identifier_claim(name)?;
        self.token(name.span, &name.text)
    }

    pub(super) fn identifier_claim(
        &self,
        name: &RawIdentifierSyntax,
    ) -> Result<Span, DeclarationError> {
        let bound = self.bound(name.span)?;
        if !super::wire::identifier(&name.text) {
            return Err(DeclarationError::malformed(Some(bound)));
        }
        if self.text.get(name.span.start as usize..name.span.end as usize)
            != Some(name.text.as_str())
        {
            return Err(DeclarationError::malformed(Some(bound)));
        }
        let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
        let start = name.span.start as usize;
        let end = name.span.end as usize;
        if (start > 0 && word(self.text.as_bytes()[start - 1]))
            || self.text.as_bytes().get(end).copied().is_some_and(word)
        {
            return Err(DeclarationError::malformed(Some(bound)));
        }
        Ok(bound)
    }

    pub(super) fn punctuation(&mut self, token: &str) -> Result<(), DeclarationError> {
        self.skip_trivia()?;
        if !self.text[self.offset..self.end].starts_with(token) {
            return Err(DeclarationError::malformed(None));
        }
        self.offset += token.len();
        Ok(())
    }

    pub(super) fn occurrence(&mut self, span: UntrustedSpan) -> Result<(), DeclarationError> {
        self.bound(span)?;
        let token = &self.text[span.start as usize..span.end as usize];
        self.token(span, token)
    }

    pub(super) fn optional_comma(&mut self) -> Result<(), DeclarationError> {
        self.skip_trivia()?;
        if self.text[self.offset..self.end].starts_with(',') {
            self.offset += 1;
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<(), DeclarationError> {
        self.skip_trivia()?;
        if self.offset == self.end { Ok(()) } else { Err(DeclarationError::malformed(None)) }
    }

    fn skip_trivia(&mut self) -> Result<(), DeclarationError> {
        self.offset += trivia_prefix(&self.text[self.offset..self.end])?;
        Ok(())
    }
}

fn trivia(text: &str) -> bool {
    trivia_prefix(text).is_ok_and(|length| length == text.len())
}

pub(super) fn line_terminator(character: char) -> bool {
    matches!(character, '\r' | '\n' | '\u{2028}' | '\u{2029}')
}

fn trivia_prefix(text: &str) -> Result<usize, DeclarationError> {
    let mut offset = 0;
    while offset < text.len() {
        let tail = &text[offset..];
        if let Some(character) = tail.chars().next().filter(|character| character.is_whitespace()) {
            offset += character.len_utf8();
        } else if tail.starts_with("//") {
            offset += tail.find(line_terminator).unwrap_or(tail.len());
        } else if tail.starts_with("/*") {
            let end = tail.find("*/").ok_or_else(|| DeclarationError::malformed(None))?;
            offset += end + 2;
        } else {
            break;
        }
    }
    Ok(offset)
}

/// Bodies are explicitly deferred, but their boundaries must be one complete balanced block.
pub(super) fn body(sources: &SourceMap, span: UntrustedSpan) -> Result<(), DeclarationError> {
    let bound = sources.verify_span(span).map_err(|_| DeclarationError::malformed(None))?;
    let source = sources.source(bound.file()).ok_or_else(|| DeclarationError::malformed(None))?;
    let text = &source.text()[span.start as usize..span.end as usize];
    if !text.starts_with('{') || !text.ends_with('}') {
        return Err(DeclarationError::malformed(Some(bound)));
    }
    let mut stack = Vec::new();
    let mut offset = 0;
    while offset < text.len() {
        offset += trivia_prefix(&text[offset..])?;
        if offset == text.len() {
            break;
        }
        let byte = text.as_bytes()[offset];
        match byte {
            b'\'' | b'"' => {
                let tail = &text[offset + 1..];
                let end = tail
                    .find(char::from(byte))
                    .ok_or_else(|| DeclarationError::malformed(Some(bound)))?;
                if tail[..end].contains(['\\', '\r', '\n']) {
                    return Err(DeclarationError::malformed(Some(bound)));
                }
                offset += end + 2;
            }
            b'{' | b'(' | b'[' => {
                if stack.len() == crate::v4::MAX_NESTING_DEPTH as usize {
                    return Err(DeclarationError { code: "ZRYNA-Y5201", span: Some(bound) });
                }
                stack.push(byte);
                offset += 1;
            }
            b'}' | b')' | b']' => {
                let expected = match byte {
                    b'}' => b'{',
                    b')' => b'(',
                    _ => b'[',
                };
                if stack.pop() != Some(expected) || (stack.is_empty() && offset + 1 != text.len()) {
                    return Err(DeclarationError::malformed(Some(bound)));
                }
                offset += 1;
            }
            _ => offset += text[offset..].chars().next().map_or(1, char::len_utf8),
        }
    }
    if stack.is_empty() { Ok(()) } else { Err(DeclarationError::malformed(Some(bound))) }
}
