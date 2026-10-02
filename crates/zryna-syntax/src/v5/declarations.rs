use std::collections::BTreeSet;

use zryna_source::SourceMap;

use super::{
    DeclarationError, RawDataDeclaration, RawDataDeclarationKind, RawFunctionSyntax, RawSourceUnit,
    RawTypeParameterList, RawTypeSyntaxKind,
    source::{self, Cursor},
    types,
};
use crate::v4::RawIdentifierSyntax;

pub(super) fn reserved(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "i32"
            | "unit"
            | "String"
            | "Vec"
            | "Shared"
            | "Weak"
            | "Borrow"
            | "BorrowMut"
            | "FixedArray"
            | "Option"
            | "Result"
            | "ZrynaValue"
            | "ZrynaStruct"
            | "ZrynaEnum"
            | "ZrynaNone"
    )
}

pub(super) fn data_name(data: &RawDataDeclaration) -> &RawIdentifierSyntax {
    match &data.kind {
        RawDataDeclarationKind::Struct { name, .. } | RawDataDeclarationKind::Enum { name, .. } => {
            name
        }
    }
}

pub(super) fn name<'a>(
    cursor: &Cursor<'_>,
    name: &'a RawIdentifierSyntax,
    names: &mut BTreeSet<&'a str>,
) -> Result<(), DeclarationError> {
    cursor.identifier_claim(name)?;
    if reserved(&name.text) || !names.insert(&name.text) {
        return Err(DeclarationError::declaration(cursor.bound(name.span)?));
    }
    Ok(())
}

fn parameters(
    cursor: &mut Cursor<'_>,
    list: Option<&RawTypeParameterList>,
    visible: &BTreeSet<&str>,
) -> Result<(), DeclarationError> {
    let Some(list) = list else { return Ok(()) };
    let bound = cursor.bound(list.span)?;
    if list.parameters.is_empty() || list.parameters.len() > super::MAX_TYPE_PARAMETERS {
        return Err(DeclarationError::declaration(bound));
    }
    if list.comma_spans.len() < list.parameters.len() - 1
        || list.comma_spans.len() > list.parameters.len()
        || list.span.start != list.less_than_span.start
        || list.span.end != list.greater_than_span.end
    {
        return Err(DeclarationError::malformed(Some(bound)));
    }
    cursor.token(list.less_than_span, "<")?;
    let mut names = BTreeSet::new();
    for (index, parameter) in list.parameters.iter().enumerate() {
        cursor.bound(parameter.span)?;
        name(cursor, &parameter.name, &mut names)?;
        if visible.contains(parameter.name.text.as_str()) {
            return Err(DeclarationError::declaration(cursor.bound(parameter.name.span)?));
        }
        if parameter.span.start != parameter.name.span.start
            || parameter.span.end != parameter.bound.span.end
        {
            return Err(DeclarationError::malformed(None));
        }
        cursor.identifier(&parameter.name)?;
        cursor.token(parameter.extends_span, "extends")?;
        cursor.identifier(&parameter.bound)?;
        if parameter.bound.text != "ZrynaValue" {
            return Err(DeclarationError::declaration(cursor.bound(parameter.bound.span)?));
        }
        if let Some(comma) = list.comma_spans.get(index) {
            cursor.token(*comma, ",")?;
        }
    }
    cursor.token(list.greater_than_span, ">")
}

pub(super) fn validate_data(
    sources: &SourceMap,
    unit: &RawSourceUnit,
    data: &RawDataDeclaration,
    visible: &BTreeSet<&str>,
) -> Result<(), DeclarationError> {
    let mut cursor = Cursor::new(sources, data.span)?;
    if let Some(export) = data.export_span {
        cursor.token(export, "export")?;
    }
    let (interface, extends, marker, open, close, spelling) = match &data.kind {
        RawDataDeclarationKind::Struct {
            interface_span,
            extends_span,
            marker_span,
            open_brace_span,
            close_brace_span,
            ..
        } => (
            *interface_span,
            *extends_span,
            *marker_span,
            *open_brace_span,
            *close_brace_span,
            "ZrynaStruct",
        ),
        RawDataDeclarationKind::Enum {
            interface_span,
            extends_span,
            marker_span,
            open_brace_span,
            close_brace_span,
            ..
        } => (
            *interface_span,
            *extends_span,
            *marker_span,
            *open_brace_span,
            *close_brace_span,
            "ZrynaEnum",
        ),
    };
    if data.span.start != data.export_span.unwrap_or(interface).start || data.span.end != close.end
    {
        return Err(DeclarationError::malformed(None));
    }
    cursor.token(interface, "interface")?;
    cursor.identifier(data_name(data))?;
    parameters(&mut cursor, data.type_parameters.as_ref(), visible)?;
    cursor.token(extends, "extends")?;
    cursor.token(marker, spelling)?;
    cursor.token(open, "{")?;
    let mut names = BTreeSet::new();
    match &data.kind {
        RawDataDeclarationKind::Struct { fields, .. } => {
            if fields.is_empty() {
                return Err(DeclarationError::declaration(cursor.bound(open)?));
            }
            for field in fields {
                cursor.bound(field.span)?;
                cursor.identifier_claim(&field.name)?;
                if !names.insert(&field.name.text) {
                    return Err(DeclarationError::declaration(cursor.bound(field.name.span)?));
                }
                cursor.identifier(&field.name)?;
                cursor.token(field.colon_span, ":")?;
                annotation(&mut cursor, unit, field.type_syntax)?;
                cursor.token(field.semicolon_span, ";")?;
                if field.span.start != field.name.span.start
                    || field.span.end != field.semicolon_span.end
                {
                    return Err(DeclarationError::malformed(None));
                }
            }
        }
        RawDataDeclarationKind::Enum { variants, .. } => {
            if variants.is_empty() {
                return Err(DeclarationError::declaration(cursor.bound(open)?));
            }
            for variant in variants {
                cursor.bound(variant.span)?;
                cursor.identifier_claim(&variant.name)?;
                if !names.insert(&variant.name.text) {
                    return Err(DeclarationError::declaration(cursor.bound(variant.name.span)?));
                }
                cursor.identifier(&variant.name)?;
                cursor.token(variant.colon_span, ":")?;
                match (variant.payload_type, variant.none_span) {
                    (Some(id), None) => annotation(&mut cursor, unit, id)?,
                    (None, Some(none)) => cursor.token(none, "ZrynaNone")?,
                    _ => return Err(DeclarationError::malformed(None)),
                }
                cursor.token(variant.semicolon_span, ";")?;
                if variant.span.start != variant.name.span.start
                    || variant.span.end != variant.semicolon_span.end
                {
                    return Err(DeclarationError::malformed(None));
                }
            }
        }
    }
    cursor.token(close, "}")?;
    cursor.finish()
}

fn annotation(
    cursor: &mut Cursor<'_>,
    unit: &RawSourceUnit,
    id: u32,
) -> Result<(), DeclarationError> {
    let node =
        unit.type_syntax.get(id as usize).ok_or_else(|| DeclarationError::malformed(None))?;
    if matches!(node.kind, RawTypeSyntaxKind::Missing) {
        return Err(DeclarationError::declaration(cursor.bound(node.span)?));
    }
    cursor.occurrence(node.span)
}

pub(super) fn validate_function(
    sources: &SourceMap,
    unit: &RawSourceUnit,
    function: &RawFunctionSyntax,
    visible: &BTreeSet<&str>,
) -> Result<(), DeclarationError> {
    let mut cursor = Cursor::new(sources, function.span)?;
    if function.span.start != function.export_span.unwrap_or(function.function_span).start
        || function.span.end != function.body.span.end
    {
        return Err(DeclarationError::malformed(None));
    }
    if let Some(export) = function.export_span {
        cursor.token(export, "export")?;
    }
    cursor.token(function.function_span, "function")?;
    cursor.identifier(&function.name)?;
    parameters(&mut cursor, function.type_parameters.as_ref(), visible)?;
    cursor.punctuation("(")?;
    let mut names = BTreeSet::new();
    for (index, parameter) in function.parameters.iter().enumerate() {
        if index != 0 {
            cursor.punctuation(",")?;
        }
        cursor.bound(parameter.span)?;
        cursor.identifier_claim(&parameter.name)?;
        if !names.insert(&parameter.name.text) {
            return Err(DeclarationError::declaration(cursor.bound(parameter.name.span)?));
        }
        cursor.identifier(&parameter.name)?;
        cursor.punctuation(":")?;
        annotation(&mut cursor, unit, parameter.type_syntax)?;
        if parameter.span.start != parameter.name.span.start
            || parameter.span.end != types::occurrence(unit, parameter.type_syntax)?.end
        {
            return Err(DeclarationError::malformed(None));
        }
    }
    // A trailing parameter comma inherits the existing source grammar.
    cursor.optional_comma()?;
    cursor.punctuation(")")?;
    cursor.punctuation(":")?;
    annotation(&mut cursor, unit, function.result_type)?;
    source::body(sources, function.body.span)?;
    cursor.occurrence(function.body.span)?;
    cursor.finish()
}
