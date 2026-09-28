//! Source-bound lexical foundation for the native protocol-v4 frontend.
//!
//! This module deliberately stops before parsing and provider selection. It retains every token
//! and trivia byte so a later parser and formatter can share one source-faithful lexical stream.

use std::fmt;

use zryna_diagnostics::Diagnostic;
use zryna_source::{FileId, NormalizedSourcePath, SourceMap, SourceMapIdentity, Span};

mod scanner;

use scanner::scan_file;

mod admission;

pub use admission::{AdmittedLexedProject, NativeSourceBytes, RawByteSpan, admit_and_lex};

/// Maximum tokens retained for one source file.
pub const MAX_TOKENS_PER_FILE: usize = 65_536;
/// Maximum trivia runs retained for one source file.
pub const MAX_TRIVIA_PER_FILE: usize = 65_536;
/// Maximum tokens plus trivia runs retained for a project.
pub const MAX_LEXEMES_PER_PROJECT: usize = 262_144;
/// Maximum aggregate source bytes accepted by the native protocol-v4 stage.
pub const MAX_SOURCE_BYTES_PER_PROJECT: usize = zryna_syntax::v4::MAX_AGGREGATE_SOURCE_BYTES;
/// Maximum malformed-input diagnostics retained before lexing fails atomically.
pub const MAX_LEXICAL_DIAGNOSTICS: usize = 256;

/// A keyword whose role is fixed by the protocol-v4 source grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Keyword {
    /// `as` import-alias keyword.
    As,
    /// `const` local-binding keyword.
    Const,
    /// `else` branch keyword.
    Else,
    /// `export` declaration modifier.
    Export,
    /// `extends` data-marker keyword.
    Extends,
    /// `false` boolean literal.
    False,
    /// `from` import-source keyword.
    From,
    /// `function` declaration keyword.
    Function,
    /// `if` branch keyword.
    If,
    /// `import` declaration keyword.
    Import,
    /// `interface` data-declaration keyword.
    Interface,
    /// `let` mutable-binding keyword.
    Let,
    /// `return` statement keyword.
    Return,
    /// `true` boolean literal.
    True,
    /// `while` loop keyword.
    While,
}

/// Source token kinds needed by the future protocol-v4 parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    /// ASCII source identifier.
    Identifier,
    /// Frozen grammar keyword.
    Keyword(Keyword),
    /// Unsigned decimal digit run; range and canonical-form checks belong to the parser.
    DecimalInteger,
    /// Complete unescaped single- or double-quoted string literal.
    StringLiteral,
    /// `{`.
    OpenBrace,
    /// `}`.
    CloseBrace,
    /// `[`.
    OpenBracket,
    /// `]`.
    CloseBracket,
    /// `(`.
    OpenParen,
    /// `)`.
    CloseParen,
    /// `:`.
    Colon,
    /// `;`.
    Semicolon,
    /// `,`.
    Comma,
    /// `.`.
    Dot,
    /// `<`.
    LessThan,
    /// `<=`.
    LessEqual,
    /// `>`.
    GreaterThan,
    /// `>=`.
    GreaterEqual,
    /// `=`.
    Equals,
    /// `=>`.
    FatArrow,
    /// `===`.
    StrictEqual,
    /// `!==`.
    StrictNotEqual,
    /// `+`.
    Plus,
    /// `-`.
    Minus,
    /// `*`.
    Asterisk,
    /// One malformed or unsupported lexical spelling.
    Invalid,
}

/// Trivia retained losslessly for formatting and source reconstruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriviaKind {
    /// One contiguous supported whitespace run.
    Whitespace,
    /// `//` comment through, but not including, its line terminator.
    LineComment,
    /// `/* ... */` comment, including both delimiters when complete.
    BlockComment,
}

/// One source-bound token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Token {
    kind: TokenKind,
    span: Span,
}

impl Token {
    /// Returns this token's lexical classification.
    #[must_use]
    pub const fn kind(self) -> TokenKind {
        self.kind
    }

    /// Returns the token's authoritative source span.
    #[must_use]
    pub const fn span(self) -> Span {
        self.span
    }
}

/// One source-bound trivia run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Trivia {
    kind: TriviaKind,
    span: Span,
}

impl Trivia {
    /// Returns this trivia run's lexical classification.
    #[must_use]
    pub const fn kind(self) -> TriviaKind {
        self.kind
    }

    /// Returns the trivia run's authoritative source span.
    #[must_use]
    pub const fn span(self) -> Span {
        self.span
    }
}

/// One ordered element of a lossless lexical stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lexeme {
    /// A parser-visible token.
    Token(Token),
    /// Formatter-visible whitespace or comment trivia.
    Trivia(Trivia),
}

impl Lexeme {
    /// Returns this lexeme's authoritative source span.
    #[must_use]
    pub const fn span(self) -> Span {
        match self {
            Self::Token(token) => token.span,
            Self::Trivia(trivia) => trivia.span,
        }
    }
}

/// Lossless lexical output for one canonical source file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexedFile {
    id: FileId,
    path: NormalizedSourcePath,
    lexemes: Vec<Lexeme>,
}

impl LexedFile {
    /// Returns the canonical map-local source-file identity.
    #[must_use]
    pub const fn id(&self) -> FileId {
        self.id
    }

    /// Returns the canonical portable source path.
    #[must_use]
    pub const fn path(&self) -> &NormalizedSourcePath {
        &self.path
    }

    /// Returns every token and trivia run in source order.
    #[must_use]
    pub fn lexemes(&self) -> &[Lexeme] {
        &self.lexemes
    }

    /// Iterates parser-visible tokens in source order.
    pub fn tokens(&self) -> impl Iterator<Item = Token> + '_ {
        self.lexemes.iter().filter_map(|lexeme| match lexeme {
            Lexeme::Token(token) => Some(*token),
            Lexeme::Trivia(_) => None,
        })
    }
}

/// Complete source-map-bound lexical output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexedProject {
    source_map_identity: SourceMapIdentity,
    files: Vec<LexedFile>,
    diagnostics: Vec<Diagnostic>,
}

impl LexedProject {
    /// Returns whether this result belongs to the exact supplied source-map authority.
    #[must_use]
    pub fn is_bound_to(&self, sources: &SourceMap) -> bool {
        self.source_map_identity == sources.identity()
    }

    /// Returns lexed files in canonical dense `FileId` order.
    #[must_use]
    pub fn files(&self) -> &[LexedFile] {
        &self.files
    }

    /// Returns recoverable malformed-input diagnostics in source order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Fatal resource failure. No partial lexical project is exposed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexError {
    diagnostic: Diagnostic,
    raw_byte_span: Option<Box<RawByteSpan>>,
}

impl LexError {
    /// Returns the fatal resource or source-boundary diagnostic.
    #[must_use]
    pub const fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }

    /// Returns the exact pre-authority byte range for a raw-input failure.
    ///
    /// This is deliberately not a [`Span`]: malformed bytes cannot belong to a [`SourceMap`].
    #[must_use]
    pub fn raw_byte_span(&self) -> Option<&RawByteSpan> {
        self.raw_byte_span.as_deref()
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.diagnostic, formatter)
    }
}

impl std::error::Error for LexError {}

/// Lexes the complete canonical source map without resolving or interpreting syntax.
///
/// Malformed source produces ordered `ZRYNA-F1501` diagnostics and `Invalid` tokens while the
/// scanner advances at an exact UTF-8 boundary. Exceeding a fixed inventory returns
/// `ZRYNA-F1502` and discards the entire candidate result.
///
/// # Errors
///
/// Returns [`LexError`] when a token, trivia, project, diagnostic, or source-boundary inventory
/// cannot be represented within its fixed limit.
pub fn lex(sources: &SourceMap) -> Result<LexedProject, LexError> {
    preflight_source_bytes(sources)?;
    let mut files = Vec::with_capacity(sources.len());
    let mut diagnostics = Vec::new();
    let mut project_lexemes = 0_usize;
    for raw_id in 0..sources.len() {
        let raw_id = u32::try_from(raw_id).map_err(|_| resource("source file id overflow"))?;
        let id = sources.verify_file_id(raw_id).map_err(|error| source_error(&error))?;
        let source = sources.source(id).ok_or_else(|| resource("source file is unavailable"))?;
        let remaining_project_lexemes = MAX_LEXEMES_PER_PROJECT - project_lexemes;
        let lexemes =
            scan_file(sources, id, source.text(), remaining_project_lexemes, &mut diagnostics)?;
        project_lexemes = project_lexemes
            .checked_add(lexemes.len())
            .ok_or_else(|| resource("project lexical inventory overflowed"))?;
        if project_lexemes > MAX_LEXEMES_PER_PROJECT {
            return Err(resource("project lexical inventory exceeds its limit"));
        }
        files.push(LexedFile { id, path: source.path().clone(), lexemes });
    }
    Ok(LexedProject { source_map_identity: sources.identity(), files, diagnostics })
}

fn preflight_source_bytes(sources: &SourceMap) -> Result<(), LexError> {
    let mut total = 0_usize;
    for raw_id in 0..sources.len() {
        let raw_id = u32::try_from(raw_id).map_err(|_| resource("source file id overflow"))?;
        let id = sources.verify_file_id(raw_id).map_err(|error| source_error(&error))?;
        let source = sources.source(id).ok_or_else(|| resource("source file is unavailable"))?;
        let next = total
            .checked_add(source.text().len())
            .ok_or_else(|| resource("project source byte inventory overflowed"))?;
        if next > MAX_SOURCE_BYTES_PER_PROJECT {
            let mut start = MAX_SOURCE_BYTES_PER_PROJECT - total;
            while !source.text().is_char_boundary(start) {
                start -= 1;
            }
            let end = start + source.text()[start..].chars().next().map_or(0, char::len_utf8);
            let span = sources
                .span(
                    id,
                    u32::try_from(start).map_err(|_| resource("source offset overflow"))?,
                    u32::try_from(end).map_err(|_| resource("source offset overflow"))?,
                )
                .map_err(|error| source_error(&error))?;
            return Err(resource_at(span, "project source bytes exceed the protocol-v4 limit"));
        }
        total = next;
    }
    Ok(())
}

pub(super) fn resource(message: &'static str) -> LexError {
    LexError {
        diagnostic: Diagnostic::error(
            "ZRYNA-F1502",
            None,
            message,
            "reduce the bounded source before native lexing",
        ),
        raw_byte_span: None,
    }
}

pub(super) fn resource_at(span: Span, message: &'static str) -> LexError {
    LexError {
        diagnostic: Diagnostic::error_at(
            "ZRYNA-F1502",
            span,
            message,
            "reduce the bounded source before native lexing",
        ),
        raw_byte_span: None,
    }
}

pub(super) fn source_error(error: &zryna_source::SourceError) -> LexError {
    LexError { diagnostic: Diagnostic::from_source_error(error), raw_byte_span: None }
}

pub(super) fn raw_error(
    code: &'static str,
    location: RawByteSpan,
    message: &'static str,
    guidance: &'static str,
) -> LexError {
    LexError {
        diagnostic: Diagnostic::error(
            code,
            Some(location.path().as_str().to_owned()),
            message,
            guidance,
        ),
        raw_byte_span: Some(Box::new(location)),
    }
}
