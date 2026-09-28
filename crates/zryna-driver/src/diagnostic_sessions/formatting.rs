//! Presentation-only scalar and bounded control-flow formatting bound to verified syntax.

use std::{fmt, ops::Range};

use zryna_frontend::{syntax_v2, syntax_v3, syntax_v4};
use zryna_source::{NormalizedSourcePath, SourceMap};

use super::{DiagnosticRevision, DiagnosticSession};

#[cfg(test)]
mod control_flow_tests;
#[cfg(test)]
mod data_ownership_tests;
mod layout;

/// A formatting failure never carries edits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormattingError {
    /// The document did not pass the admitted formatting analysis.
    Unavailable,
    /// The requested revision is no longer active.
    Stale,
    /// The selection is invalid or cuts through a top-level syntax unit.
    Range,
    /// The bounded formatting result is too large.
    Limit,
}

impl FormattingError {
    /// Returns the stable formatter diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "ZRYNA-D4001",
            Self::Stale => "ZRYNA-D4002",
            Self::Range => "ZRYNA-D4003",
            Self::Limit => "ZRYNA-D4004",
        }
    }
}

impl fmt::Display for FormattingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: {}",
            self.code(),
            match self {
                Self::Unavailable => "formatting requires verified syntax and semantics",
                Self::Stale => "formatting revision is no longer active",
                Self::Range => "formatting range must contain complete syntax units",
                Self::Limit => "formatting result exceeds the response limit",
            }
        )
    }
}

impl std::error::Error for FormattingError {}

/// One replacement in the requested immutable document's UTF-8 coordinates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormattingEdit {
    /// Inclusive start byte in the original source.
    pub start: u32,
    /// Exclusive end byte in the original source.
    pub end: u32,
    /// Replacement source text. Applying edits belongs to the client.
    pub text: String,
}

#[derive(Debug)]
pub(super) struct FormattingDocument {
    path: String,
    complete: Result<String, FormattingError>,
    functions: Vec<Range<u32>>,
    control_flow: bool,
    data_ownership: bool,
}

impl FormattingDocument {
    pub(super) fn prepare(
        syntax: &syntax_v2::ProjectSyntaxSnapshot,
        sources: &SourceMap,
    ) -> Option<Self> {
        if !syntax.is_bound_to(sources) {
            return None;
        }
        let [file] = syntax.files() else { return None };
        let source = sources.source(file.id())?;
        let complete = layout::format(source.text())?;
        let complete = if complete.len() > super::MAX_RESPONSE_BYTES / 8 {
            Err(FormattingError::Limit)
        } else {
            Ok(complete)
        };
        Some(Self {
            path: file.path().as_str().to_owned(),
            complete,
            functions: file.functions().iter().map(|f| f.span().start()..f.span().end()).collect(),
            control_flow: false,
            data_ownership: false,
        })
    }

    pub(super) fn prepare_control_flow(
        syntax: &syntax_v3::ProjectSyntaxSnapshot,
        sources: &SourceMap,
    ) -> Option<Self> {
        if !syntax.is_bound_to(sources) {
            return None;
        }
        let [file] = syntax.files() else { return None };
        if !file.imports().is_empty() {
            return None;
        }
        let source = sources.source(file.id())?;
        let complete =
            match layout::format_control_flow_bounded(source.text(), super::MAX_RESPONSE_BYTES / 8)
            {
                Ok(complete) => Ok(complete),
                Err(layout::LayoutError::Limit) => Err(FormattingError::Limit),
                Err(layout::LayoutError::Invalid) => return None,
            };
        Some(Self {
            path: file.path().as_str().to_owned(),
            complete,
            functions: file.functions().iter().map(|f| f.span().start()..f.span().end()).collect(),
            control_flow: true,
            data_ownership: false,
        })
    }

    pub(super) fn prepare_data_ownership(
        syntax: &syntax_v4::ProjectSyntaxSnapshot,
        sources: &SourceMap,
    ) -> Option<Vec<Self>> {
        if !syntax.is_bound_to(sources) {
            return None;
        }
        syntax
            .files()
            .iter()
            .map(|file| {
                let source = sources.source(file.id())?;
                let complete = match layout::format_data_ownership_bounded(
                    source.text(),
                    super::MAX_RESPONSE_BYTES / 8,
                ) {
                    Ok(complete) => Ok(complete),
                    Err(layout::LayoutError::Limit) => Err(FormattingError::Limit),
                    Err(layout::LayoutError::Invalid) => return None,
                };
                let mut units = file
                    .imports()
                    .iter()
                    .map(|item| item.span.start..item.span.end)
                    .chain(
                        file.data_declarations().iter().map(|item| item.span.start..item.span.end),
                    )
                    .chain(file.functions().iter().map(|item| item.span.start..item.span.end))
                    .collect::<Vec<_>>();
                units.sort_by_key(|span| span.start);
                Some(Self {
                    path: file.path().as_str().to_owned(),
                    complete,
                    functions: units,
                    control_flow: false,
                    data_ownership: true,
                })
            })
            .collect()
    }

    pub(super) fn cache_bytes(&self) -> usize {
        self.path.len() + self.complete.as_ref().map_or(0, String::len) + self.functions.len() * 8
    }
}

impl DiagnosticSession {
    /// Returns edits only for the active semantically accepted formatting revision.
    ///
    /// A range may surround whole top-level units and trivia, but may not intersect a partial
    /// unit. Edits stay inside the selection and preserve all text outside each selected unit.
    ///
    /// # Errors
    ///
    /// Returns stable rejection for stale authority, unavailable analysis, invalid ranges or limits.
    pub fn format_source(
        &self,
        revision: DiagnosticRevision,
        path: &str,
        range: Option<Range<u32>>,
    ) -> Result<Vec<FormattingEdit>, FormattingError> {
        let record = self
            .retained
            .back()
            .filter(|r| r.description == revision)
            .ok_or(FormattingError::Stale)?;
        let document = record
            .formatting
            .iter()
            .find(|f| f.path == path)
            .ok_or(FormattingError::Unavailable)?;
        let complete = document.complete.as_ref().map_err(|error| *error)?;
        let normalized =
            NormalizedSourcePath::new(path.to_owned()).map_err(|_| FormattingError::Unavailable)?;
        let id = record.sources.file_id(&normalized).ok_or(FormattingError::Unavailable)?;
        let source = record.sources.source(id).ok_or(FormattingError::Unavailable)?.text();
        let end = u32::try_from(source.len()).map_err(|_| FormattingError::Limit)?;
        let Some(range) = range else {
            return Ok(if complete == source {
                Vec::new()
            } else {
                vec![FormattingEdit { start: 0, end, text: complete.clone() }]
            });
        };
        record.sources.span(id, range.start, range.end).map_err(|_| FormattingError::Range)?;
        if range.is_empty() {
            return Ok(Vec::new());
        }
        let mut edits = Vec::new();
        for function in &document.functions {
            if range.start < function.end && range.end > function.start {
                if range.start > function.start || range.end < function.end {
                    return Err(FormattingError::Range);
                }
                let original = &source[function.start as usize..function.end as usize];
                let text = if document.data_ownership {
                    Some(
                        layout::format_data_ownership_bounded(
                            original,
                            super::MAX_RESPONSE_BYTES / 8,
                        )
                        .map_err(|error| match error {
                            layout::LayoutError::Limit => FormattingError::Limit,
                            layout::LayoutError::Invalid => FormattingError::Unavailable,
                        })?,
                    )
                } else if document.control_flow {
                    layout::format_control_flow(original)
                } else {
                    layout::format(original)
                }
                .ok_or(FormattingError::Unavailable)?;
                let text = text.strip_suffix('\n').unwrap_or(&text);
                if original != text {
                    edits.push(FormattingEdit {
                        start: function.start,
                        end: function.end,
                        text: text.to_owned(),
                    });
                }
            }
        }
        if edits.len()
            > usize::try_from(super::MAX_QUERY_RESULTS).map_err(|_| FormattingError::Limit)?
        {
            return Err(FormattingError::Limit);
        }
        Ok(edits)
    }
}
