use serde::{Deserialize, Serialize};
use zryna_source::UntrustedSpan;

use super::RawTypeArgumentList;
use crate::v4::RawIdentifierSyntax;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawTypeSyntax {
    pub span: UntrustedSpan,
    pub kind: RawTypeSyntaxKind,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RawTypeSyntaxKind {
    Missing,
    Named {
        name: RawIdentifierSyntax,
    },
    String {
        keyword_span: UntrustedSpan,
    },
    Vec {
        keyword_span: UntrustedSpan,
        less_than_span: UntrustedSpan,
        argument: u32,
        greater_than_span: UntrustedSpan,
    },
    Shared {
        keyword_span: UntrustedSpan,
        less_than_span: UntrustedSpan,
        argument: u32,
        greater_than_span: UntrustedSpan,
    },
    Weak {
        keyword_span: UntrustedSpan,
        less_than_span: UntrustedSpan,
        argument: u32,
        greater_than_span: UntrustedSpan,
    },
    Borrow {
        keyword_span: UntrustedSpan,
        less_than_span: UntrustedSpan,
        argument: u32,
        greater_than_span: UntrustedSpan,
    },
    BorrowMut {
        keyword_span: UntrustedSpan,
        less_than_span: UntrustedSpan,
        argument: u32,
        greater_than_span: UntrustedSpan,
    },
    Application {
        name: RawIdentifierSyntax,
        type_arguments: RawTypeArgumentList,
    },
    FixedArray {
        keyword_span: UntrustedSpan,
        less_than_span: UntrustedSpan,
        element: u32,
        comma_span: UntrustedSpan,
        length_span: UntrustedSpan,
        length_spelling: String,
        length: u32,
        greater_than_span: UntrustedSpan,
    },
}

use super::{DeclarationError, RawSourceUnit, source::Cursor};
use zryna_source::SourceMap;

pub(super) fn occurrence(unit: &RawSourceUnit, id: u32) -> Result<UntrustedSpan, DeclarationError> {
    unit.type_syntax
        .get(id as usize)
        .map(|node| node.span)
        .ok_or_else(|| DeclarationError::malformed(None))
}

pub(super) fn validate(sources: &SourceMap, unit: &RawSourceUnit) -> Result<(), DeclarationError> {
    let mut depths = Vec::with_capacity(unit.type_syntax.len());
    for (index, node) in unit.type_syntax.iter().enumerate() {
        let mut cursor = Cursor::new(sources, node.span)?;
        if node.span.file != unit.id {
            return Err(DeclarationError::malformed(None));
        }
        let (start, end) = endpoints(node)?;
        if node.span.start != start || node.span.end != end {
            return Err(DeclarationError::malformed(None));
        }
        let mut depth = 0;
        let mut child = |cursor: &mut Cursor<'_>, id: u32| -> Result<(), DeclarationError> {
            if id as usize >= index {
                return Err(DeclarationError::malformed(None));
            }
            depth = depth.max(depths[id as usize] + 1);
            cursor.occurrence(occurrence(unit, id)?)
        };
        match &node.kind {
            RawTypeSyntaxKind::Missing => {}
            RawTypeSyntaxKind::Named { name } => cursor.identifier(name)?,
            RawTypeSyntaxKind::String { keyword_span } => cursor.token(*keyword_span, "String")?,
            RawTypeSyntaxKind::Application { name, type_arguments } => {
                cursor.identifier(name)?;
                arguments(&mut cursor, type_arguments, &mut child)?;
            }
            RawTypeSyntaxKind::FixedArray {
                keyword_span,
                less_than_span,
                element,
                comma_span,
                length_span,
                length_spelling,
                length,
                greater_than_span,
            } => {
                cursor.token(*keyword_span, "FixedArray")?;
                cursor.token(*less_than_span, "<")?;
                child(&mut cursor, *element)?;
                cursor.token(*comma_span, ",")?;
                cursor.token(*length_span, length_spelling)?;
                if length_spelling.parse::<u32>().ok() != Some(*length) || *length > 1_048_576 {
                    return Err(DeclarationError::malformed(Some(cursor.bound(*length_span)?)));
                }
                cursor.token(*greater_than_span, ">")?;
            }
            RawTypeSyntaxKind::Vec {
                keyword_span,
                less_than_span,
                argument,
                greater_than_span,
            }
            | RawTypeSyntaxKind::Shared {
                keyword_span,
                less_than_span,
                argument,
                greater_than_span,
            }
            | RawTypeSyntaxKind::Weak {
                keyword_span,
                less_than_span,
                argument,
                greater_than_span,
            }
            | RawTypeSyntaxKind::Borrow {
                keyword_span,
                less_than_span,
                argument,
                greater_than_span,
            }
            | RawTypeSyntaxKind::BorrowMut {
                keyword_span,
                less_than_span,
                argument,
                greater_than_span,
            } => {
                let keyword = match &node.kind {
                    RawTypeSyntaxKind::Vec { .. } => "Vec",
                    RawTypeSyntaxKind::Shared { .. } => "Shared",
                    RawTypeSyntaxKind::Weak { .. } => "Weak",
                    RawTypeSyntaxKind::Borrow { .. } => "Borrow",
                    _ => "BorrowMut",
                };
                cursor.token(*keyword_span, keyword)?;
                cursor.token(*less_than_span, "<")?;
                child(&mut cursor, *argument)?;
                cursor.token(*greater_than_span, ">")?;
            }
        }
        cursor.finish()?;
        if depth > crate::v4::MAX_NESTING_DEPTH {
            return Err(DeclarationError { code: "ZRYNA-Y5201", span: None });
        }
        depths.push(depth);
    }
    Ok(())
}

pub(super) fn arguments(
    cursor: &mut Cursor<'_>,
    list: &RawTypeArgumentList,
    child: &mut impl FnMut(&mut Cursor<'_>, u32) -> Result<(), DeclarationError>,
) -> Result<(), DeclarationError> {
    cursor.bound(list.span)?;
    if list.arguments.is_empty()
        || list.arguments.len() > super::MAX_TYPE_ARGUMENTS
        || list.comma_spans.len() < list.arguments.len() - 1
        || list.comma_spans.len() > list.arguments.len()
        || list.span.start != list.less_than_span.start
        || list.span.end != list.greater_than_span.end
    {
        return Err(DeclarationError::malformed(None));
    }
    cursor.token(list.less_than_span, "<")?;
    for (index, argument) in list.arguments.iter().enumerate() {
        child(cursor, *argument)?;
        if let Some(comma) = list.comma_spans.get(index) {
            cursor.token(*comma, ",")?;
        }
    }
    cursor.token(list.greater_than_span, ">")
}

fn endpoints(node: &RawTypeSyntax) -> Result<(u32, u32), DeclarationError> {
    Ok(match &node.kind {
        RawTypeSyntaxKind::Missing => (node.span.start, node.span.start),
        RawTypeSyntaxKind::Named { name } => (name.span.start, name.span.end),
        RawTypeSyntaxKind::String { keyword_span } => (keyword_span.start, keyword_span.end),
        RawTypeSyntaxKind::Application { name, type_arguments } => {
            if matches!(
                name.text.as_str(),
                "String" | "Vec" | "Shared" | "Weak" | "Borrow" | "BorrowMut" | "FixedArray"
            ) {
                return Err(DeclarationError::malformed(None));
            }
            (name.span.start, type_arguments.span.end)
        }
        RawTypeSyntaxKind::Vec { keyword_span, greater_than_span, .. }
        | RawTypeSyntaxKind::Shared { keyword_span, greater_than_span, .. }
        | RawTypeSyntaxKind::Weak { keyword_span, greater_than_span, .. }
        | RawTypeSyntaxKind::Borrow { keyword_span, greater_than_span, .. }
        | RawTypeSyntaxKind::BorrowMut { keyword_span, greater_than_span, .. }
        | RawTypeSyntaxKind::FixedArray { keyword_span, greater_than_span, .. } => {
            (keyword_span.start, greater_than_span.end)
        }
    })
}
