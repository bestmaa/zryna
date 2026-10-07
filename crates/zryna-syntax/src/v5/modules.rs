use std::collections::BTreeSet;

use zryna_source::{SourceMap, UntrustedSpan};

use super::declarations::{data_name, name, reserved, validate_data, validate_function};
use super::{DeclarationError, PROTOCOL_VERSION, RawProjectSyntaxSnapshot, source::Cursor, types};
use crate::v4::RawImportSyntax;

/// Checks the complete top-level inventory, data bodies, type occurrences and function headers.
///
/// Function bodies remain untrusted balanced source ranges. This check returns no opaque
/// executable snapshot and must not replace complete body/arena, module or semantic verification.
///
/// # Errors
/// Returns the first source-ordered declaration or malformed source-claim failure.
pub fn validate_declarations(
    sources: &SourceMap,
    raw: &RawProjectSyntaxSnapshot,
) -> Result<(), DeclarationError> {
    if raw.schema_version != PROTOCOL_VERSION || raw.files.len() != sources.len() {
        return Err(DeclarationError::malformed(None));
    }
    super::resources::preflight(sources, raw)?;
    for (index, unit) in raw.files.iter().enumerate() {
        if unit.id as usize != index {
            return Err(DeclarationError::malformed(None));
        }
        let file =
            sources.verify_file_id(unit.id).map_err(|_| DeclarationError::malformed(None))?;
        let source = sources.source(file).ok_or_else(|| DeclarationError::malformed(None))?;
        if source.path().as_str() != unit.path {
            return Err(DeclarationError::malformed(None));
        }
        let end =
            u32::try_from(source.text().len()).map_err(|_| DeclarationError::malformed(None))?;
        let mut cursor = Cursor::new(sources, UntrustedSpan { file: unit.id, start: 0, end })?;
        types::validate(sources, unit)?;
        for import in &unit.imports {
            validate_import(sources, import, &mut cursor)?;
        }
        let mut names = BTreeSet::new();
        let visible_types = unit
            .data_declarations
            .iter()
            .map(data_name)
            .map(|name| name.text.as_str())
            .collect::<BTreeSet<_>>();
        let mut data_index = 0;
        let mut function_index = 0;
        // Merge without sorting: a reordered array, omitted declaration or trailing token rejects.
        while data_index < unit.data_declarations.len() || function_index < unit.functions.len() {
            let data = unit.data_declarations.get(data_index);
            let function = unit.functions.get(function_index);
            if data.is_some_and(|data| {
                function.is_none_or(|function| data.span.start < function.span.start)
            }) {
                let data = data.ok_or_else(|| DeclarationError::malformed(None))?;
                name(&cursor, data_name(data), &mut names)?;
                validate_data(sources, unit, data, &visible_types)?;
                cursor.occurrence(data.span)?;
                data_index += 1;
            } else {
                let function = function.ok_or_else(|| DeclarationError::malformed(None))?;
                name(&cursor, &function.name, &mut names)?;
                validate_function(sources, unit, function, &visible_types)?;
                cursor.occurrence(function.span)?;
                function_index += 1;
            }
        }
        cursor.finish()?;
    }
    Ok(())
}

fn validate_import(
    sources: &SourceMap,
    import: &RawImportSyntax,
    outer: &mut Cursor<'_>,
) -> Result<(), DeclarationError> {
    let mut cursor = Cursor::new(sources, import.span)?;
    cursor.token(import.import_span, "import")?;
    cursor.punctuation("{")?;
    if import.bindings.is_empty() {
        return Err(DeclarationError::malformed(None));
    }
    for (index, binding) in import.bindings.iter().enumerate() {
        if index != 0 {
            cursor.punctuation(",")?;
        }
        cursor.bound(binding.span)?;
        cursor.identifier_claim(&binding.imported)?;
        cursor.identifier_claim(&binding.local)?;
        if reserved(&binding.imported.text) || reserved(&binding.local.text) {
            return Err(DeclarationError::declaration(cursor.bound(binding.local.span)?));
        }
        cursor.identifier(&binding.imported)?;
        if let Some(alias) = binding.as_span {
            cursor.token(alias, "as")?;
            cursor.identifier(&binding.local)?;
        } else if binding.imported != binding.local {
            return Err(DeclarationError::malformed(None));
        }
        if binding.span.start != binding.imported.span.start
            || binding.span.end != binding.local.span.end
        {
            return Err(DeclarationError::malformed(None));
        }
    }
    cursor.optional_comma()?;
    cursor.punctuation("}")?;
    cursor.token(import.from_span, "from")?;
    let token = cursor.bound(import.specifier.token_span)?;
    let value = cursor.bound(import.specifier.value_span)?;
    if token.start().checked_add(1) != Some(value.start())
        || token.end().checked_sub(1) != Some(value.end())
    {
        return Err(DeclarationError::malformed(None));
    }
    let single = format!("'{}'", import.specifier.text);
    let double = format!("\"{}\"", import.specifier.text);
    let source = sources.source(token.file()).ok_or_else(|| DeclarationError::malformed(None))?;
    let spelling = &source.text()[token.start() as usize..token.end() as usize];
    if spelling != single && spelling != double {
        return Err(DeclarationError::malformed(Some(token)));
    }
    cursor.token(import.specifier.token_span, spelling)?;
    cursor.token(import.semicolon_span, ";")?;
    cursor.finish()?;
    outer.occurrence(import.span)
}
