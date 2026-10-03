//! Complete significant-source consumption; a child claim never authenticates its contents.

use zryna_source::{SourceMap, Span, UntrustedSpan};

use super::{DeclarationError, RawSourceUnit, RawTypeArgumentList, RawTypeSyntaxKind, arena};
use crate::v4::RawIdentifierSyntax;

#[derive(Clone, Copy)]
pub(super) enum Role {
    FunctionBinding,
    DataBinding,
    FunctionTypeParameter,
    DataTypeParameter,
    ValueBinding,
    LocalBinding,
    Runtime,
    Assignment,
    TypeHead,
    ApplicationHead,
}

#[derive(Clone, Copy)]
pub(super) struct Context {
    external_module: bool,
    in_function: bool,
}

fn reserved_runtime(text: &str) -> bool {
    matches!(
        text,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
    )
}

fn strict_word(text: &str) -> bool {
    matches!(
        text,
        "implements"
            | "interface"
            | "let"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "static"
            | "yield"
    )
}

fn type_diversion(text: &str) -> bool {
    matches!(
        text,
        "any"
            | "unknown"
            | "string"
            | "number"
            | "bigint"
            | "symbol"
            | "boolean"
            | "undefined"
            | "never"
            | "object"
            | "true"
            | "false"
            | "null"
            | "void"
            | "this"
            | "typeof"
            | "import"
            | "new"
            | "keyof"
            | "unique"
            | "readonly"
            | "infer"
    )
}

impl Context {
    /// Prefix facts authenticate module evidence; complete inventory/EOF coverage is still required.
    pub(super) fn authenticate(
        sources: &SourceMap,
        unit: &RawSourceUnit,
    ) -> Result<Self, DeclarationError> {
        let mut external_module = false;
        for import in &unit.imports {
            if import.span.file != unit.id || import.span.start != import.import_span.start {
                return Err(arena::malformed());
            }
            Cursor::new(sources, import.span)?.token(import.import_span, "import")?;
            external_module = true;
        }
        for (span, export) in unit
            .data_declarations
            .iter()
            .map(|data| (data.span, data.export_span))
            .chain(unit.functions.iter().map(|function| (function.span, function.export_span)))
        {
            if let Some(export) = export {
                if span.file != unit.id || span.start != export.start {
                    return Err(arena::malformed());
                }
                Cursor::new(sources, span)?.token(export, "export")?;
                external_module = true;
            }
        }
        Ok(Self { external_module, in_function: false })
    }

    pub(super) const fn function(self) -> Self {
        Self { in_function: true, ..self }
    }

    pub(super) fn allows(self, text: &str, role: Role) -> bool {
        if matches!(role, Role::TypeHead | Role::ApplicationHead) {
            return !(type_diversion(text)
                || matches!(role, Role::ApplicationHead) && text == "function"
                || self.external_module
                    && (strict_word(text) || !self.in_function && text == "await"));
        }
        if reserved_runtime(text) || (self.external_module && strict_word(text)) {
            return false;
        }
        match role {
            Role::FunctionBinding => {
                !(self.external_module && matches!(text, "await" | "eval" | "arguments"))
            }
            Role::DataBinding | Role::DataTypeParameter => {
                !(self.external_module && text == "await")
            }
            Role::FunctionTypeParameter | Role::Runtime => true,
            Role::ValueBinding | Role::Assignment => {
                !(self.external_module && matches!(text, "eval" | "arguments"))
            }
            Role::LocalBinding => {
                text != "let" && !(self.external_module && matches!(text, "eval" | "arguments"))
            }
            Role::TypeHead | Role::ApplicationHead => unreachable!(),
        }
    }

    pub(super) fn identifier(
        self,
        cursor: &mut Cursor<'_>,
        name: &RawIdentifierSyntax,
        role: Role,
    ) -> Result<(), DeclarationError> {
        cursor.identifier(name)?;
        if !self.allows(&name.text, role) {
            return Err(DeclarationError::malformed(Some(cursor.bound(name.span)?)));
        }
        Ok(())
    }
}

pub(super) struct Cursor<'a> {
    inner: super::source::Cursor<'a>,
    sources: &'a SourceMap,
    span: UntrustedSpan,
}

impl<'a> Cursor<'a> {
    pub(super) fn new_from(parent: &Self, span: UntrustedSpan) -> Result<Self, DeclarationError> {
        parent.bound(span)?;
        Self::new(parent.sources, span)
    }

    pub(super) fn new(
        sources: &'a SourceMap,
        span: UntrustedSpan,
    ) -> Result<Self, DeclarationError> {
        Ok(Self { inner: super::source::Cursor::new(sources, span)?, sources, span })
    }

    pub(super) fn bound(&self, span: UntrustedSpan) -> Result<Span, DeclarationError> {
        let bound = self.inner.bound(span)?;
        if span.start < self.span.start {
            return Err(DeclarationError::malformed(Some(bound)));
        }
        Ok(bound)
    }

    pub(super) fn text(&self, span: UntrustedSpan) -> Result<&'a str, DeclarationError> {
        let bound = self.bound(span)?;
        let source = self.sources.source(bound.file()).ok_or_else(arena::malformed)?;
        source.text().get(span.start as usize..span.end as usize).ok_or_else(arena::malformed)
    }

    pub(super) fn token(
        &mut self,
        span: UntrustedSpan,
        text: &str,
    ) -> Result<(), DeclarationError> {
        self.bound(span)?;
        self.inner.token(span, text)
    }

    pub(super) fn identifier(
        &mut self,
        name: &RawIdentifierSyntax,
    ) -> Result<(), DeclarationError> {
        self.bound(name.span)?;
        self.inner.identifier(name)
    }

    pub(super) fn punctuation(&mut self, text: &str) -> Result<(), DeclarationError> {
        self.inner.punctuation(text)
    }

    pub(super) fn child(&mut self, span: UntrustedSpan) -> Result<(), DeclarationError> {
        self.bound(span)?;
        self.inner.occurrence(span)
    }

    pub(super) fn comma(&mut self) -> Result<(), DeclarationError> {
        self.inner.optional_comma()
    }

    pub(super) fn finish(self) -> Result<(), DeclarationError> {
        self.inner.finish()
    }

    pub(super) fn annotation(
        &mut self,
        unit: &RawSourceUnit,
        id: u32,
    ) -> Result<(), DeclarationError> {
        self.child(super::types::occurrence(unit, id)?)
    }

    pub(super) fn type_arguments(
        &mut self,
        unit: &RawSourceUnit,
        list: Option<&RawTypeArgumentList>,
    ) -> Result<(), DeclarationError> {
        let Some(list) = list else { return Ok(()) };
        self.bound(list.span)?;
        if list.arguments.is_empty()
            || list.arguments.len() > super::MAX_TYPE_ARGUMENTS
            || list.comma_spans.len() < list.arguments.len() - 1
            || list.comma_spans.len() > list.arguments.len()
            || list.span.start != list.less_than_span.start
            || list.span.end != list.greater_than_span.end
        {
            return Err(arena::malformed());
        }
        self.token(list.less_than_span, "<")?;
        for (index, id) in list.arguments.iter().enumerate() {
            if missing(unit, *id)? {
                return Err(arena::malformed());
            }
            self.annotation(unit, *id)?;
            if let Some(comma) = list.comma_spans.get(index) {
                self.token(*comma, ",")?;
            }
        }
        self.token(list.greater_than_span, ">")
    }
}

pub(super) fn missing(unit: &RawSourceUnit, id: u32) -> Result<bool, DeclarationError> {
    let node = unit.type_syntax.get(id as usize).ok_or_else(arena::malformed)?;
    Ok(matches!(node.kind, RawTypeSyntaxKind::Missing))
}

fn parameters(
    cursor: &mut Cursor<'_>,
    list: Option<&super::RawTypeParameterList>,
    context: Context,
    role: Role,
) -> Result<(), DeclarationError> {
    let Some(list) = list else { return Ok(()) };
    cursor.bound(list.span)?;
    if list.parameters.is_empty()
        || list.parameters.len() > super::MAX_TYPE_PARAMETERS
        || list.comma_spans.len() < list.parameters.len() - 1
        || list.comma_spans.len() > list.parameters.len()
        || list.span.start != list.less_than_span.start
        || list.span.end != list.greater_than_span.end
    {
        return Err(arena::malformed());
    }
    cursor.token(list.less_than_span, "<")?;
    for (index, parameter) in list.parameters.iter().enumerate() {
        cursor.bound(parameter.span)?;
        if parameter.span.start != parameter.name.span.start
            || parameter.span.end != parameter.bound.span.end
        {
            return Err(arena::malformed());
        }
        context.identifier(cursor, &parameter.name, role)?;
        cursor.token(parameter.extends_span, "extends")?;
        context.identifier(cursor, &parameter.bound, Role::TypeHead)?;
        if let Some(comma) = list.comma_spans.get(index) {
            cursor.token(*comma, ",")?;
        }
    }
    cursor.token(list.greater_than_span, ">")
}

pub(super) fn function(
    sources: &SourceMap,
    unit: &RawSourceUnit,
    function: &super::RawFunctionSyntax,
    context: Context,
) -> Result<(), DeclarationError> {
    let mut cursor = Cursor::new(sources, function.span)?;
    if function.span.start != function.export_span.unwrap_or(function.function_span).start
        || function.span.end != function.body.span.end
    {
        return Err(arena::malformed());
    }
    if let Some(export) = function.export_span {
        cursor.token(export, "export")?;
    }
    cursor.token(function.function_span, "function")?;
    context.identifier(&mut cursor, &function.name, Role::FunctionBinding)?;
    let context = context.function();
    parameters(
        &mut cursor,
        function.type_parameters.as_ref(),
        context,
        Role::FunctionTypeParameter,
    )?;
    cursor.punctuation("(")?;
    for (index, parameter) in function.parameters.iter().enumerate() {
        if index != 0 {
            cursor.punctuation(",")?;
        }
        cursor.bound(parameter.span)?;
        context.identifier(&mut cursor, &parameter.name, Role::ValueBinding)?;
        cursor.punctuation(":")?;
        cursor.annotation(unit, parameter.type_syntax)?;
        if parameter.span.start != parameter.name.span.start
            || parameter.span.end != super::types::occurrence(unit, parameter.type_syntax)?.end
        {
            return Err(arena::malformed());
        }
    }
    if !function.parameters.is_empty() {
        cursor.comma()?;
    }
    cursor.punctuation(")")?;
    cursor.punctuation(":")?;
    cursor.annotation(unit, function.result_type)?;
    cursor.child(function.body.span)?;
    cursor.finish()
}

pub(super) fn data(
    sources: &SourceMap,
    unit: &RawSourceUnit,
    data: &super::RawDataDeclaration,
    context: Context,
) -> Result<(), DeclarationError> {
    use super::RawDataDeclarationKind::{Enum, Struct};
    let mut cursor = Cursor::new(sources, data.span)?;
    let (interface, extends, marker, open, close, spelling) = match &data.kind {
        Struct {
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
        Enum {
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
        return Err(arena::malformed());
    }
    if let Some(export) = data.export_span {
        cursor.token(export, "export")?;
    }
    cursor.token(interface, "interface")?;
    context.identifier(&mut cursor, super::declarations::data_name(data), Role::DataBinding)?;
    parameters(&mut cursor, data.type_parameters.as_ref(), context, Role::DataTypeParameter)?;
    cursor.token(extends, "extends")?;
    cursor.token(marker, spelling)?;
    cursor.token(open, "{")?;
    match &data.kind {
        Struct { fields, .. } => {
            for field in fields {
                cursor.bound(field.span)?;
                cursor.identifier(&field.name)?;
                cursor.token(field.colon_span, ":")?;
                cursor.annotation(unit, field.type_syntax)?;
                cursor.token(field.semicolon_span, ";")?;
                if field.span.start != field.name.span.start
                    || field.span.end != field.semicolon_span.end
                {
                    return Err(arena::malformed());
                }
            }
        }
        Enum { variants, .. } => {
            for variant in variants {
                cursor.bound(variant.span)?;
                cursor.identifier(&variant.name)?;
                cursor.token(variant.colon_span, ":")?;
                match (variant.payload_type, variant.none_span) {
                    (Some(id), None) => cursor.annotation(unit, id)?,
                    (None, Some(span)) => cursor.token(span, "ZrynaNone")?,
                    _ => return Err(arena::malformed()),
                }
                cursor.token(variant.semicolon_span, ";")?;
                if variant.span.start != variant.name.span.start
                    || variant.span.end != variant.semicolon_span.end
                {
                    return Err(arena::malformed());
                }
            }
        }
    }
    cursor.token(close, "}")?;
    cursor.finish()
}
