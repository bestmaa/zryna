use std::fmt;

use zryna_source::Span;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyntaxDecodeError {
    ResponseTooLarge { actual: usize, limit: usize },
    InvalidSnapshot,
}

impl SyntaxDecodeError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ResponseTooLarge { .. } => "ZRYNA-Y5201",
            Self::InvalidSnapshot => "ZRYNA-Y5001",
        }
    }
}

impl fmt::Display for SyntaxDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResponseTooLarge { actual, limit } => {
                write!(formatter, "protocol-v5 response contains {actual} bytes; limit {limit}")
            }
            Self::InvalidSnapshot => formatter.write_str("invalid closed protocol-v5 JSON"),
        }
    }
}

impl std::error::Error for SyntaxDecodeError {}

/// A failed declaration check; a location is present only when `SourceMap` authenticated it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationError {
    pub code: &'static str,
    pub span: Option<Span>,
}

impl DeclarationError {
    pub(super) const fn malformed(span: Option<Span>) -> Self {
        Self { code: "ZRYNA-Y5001", span }
    }

    pub(super) const fn declaration(span: Span) -> Self {
        Self { code: "ZRYNA-D7001", span: Some(span) }
    }
}
