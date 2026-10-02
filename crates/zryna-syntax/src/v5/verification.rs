//! Admission requires the complete raw/source barrier across all files before D diagnostics.

use super::{
    DeclarationError, RawProjectSyntaxSnapshot, RawSourceUnit, VerifiedProjectSyntaxV5, arena,
    coverage::{self, Context, Cursor, Role},
    diagnostic_order::Errors,
};
use zryna_source::{SourceMap, UntrustedSpan};

pub(super) struct CompleteSyntaxProof(());

fn import(
    sources: &SourceMap,
    unit: &RawSourceUnit,
    cursor: &mut Cursor<'_>,
    import: &crate::v4::RawImportSyntax,
    context: Context,
) -> Result<(), DeclarationError> {
    let mut item = Cursor::new(sources, import.span)?;
    if import.span.file != unit.id
        || import.span.start != import.import_span.start
        || import.span.end != import.semicolon_span.end
        || import.bindings.is_empty()
    {
        return Err(arena::malformed());
    }
    item.token(import.import_span, "import")?;
    item.punctuation("{")?;
    for (index, binding) in import.bindings.iter().enumerate() {
        if index != 0 {
            item.punctuation(",")?;
        }
        item.bound(binding.span)?;
        item.identifier(&binding.imported)?;
        if let Some(alias) = binding.as_span {
            item.token(alias, "as")?;
            context.identifier(&mut item, &binding.local, Role::FunctionBinding)?;
        } else if binding.imported != binding.local {
            return Err(arena::malformed());
        } else if !context.allows(&binding.local.text, Role::FunctionBinding) {
            return Err(DeclarationError::malformed(Some(item.bound(binding.local.span)?)));
        }
        if binding.span.start != binding.imported.span.start
            || binding.span.end != binding.local.span.end
        {
            return Err(arena::malformed());
        }
    }
    item.comma()?;
    item.punctuation("}")?;
    item.token(import.from_span, "from")?;
    let token = item.bound(import.specifier.token_span)?;
    let value = item.bound(import.specifier.value_span)?;
    if token.start().checked_add(1) != Some(value.start())
        || token.end().checked_sub(1) != Some(value.end())
    {
        return Err(arena::malformed());
    }
    let spelling = item.text(import.specifier.token_span)?;
    if spelling != format!("'{}'", import.specifier.text)
        && spelling != format!("\"{}\"", import.specifier.text)
    {
        return Err(arena::malformed());
    }
    if !super::wire::valid(
        &serde_json::json!({ "text": import.specifier.text, "token_span": import.specifier.token_span }),
    ) {
        return Err(arena::malformed());
    }
    item.token(import.specifier.token_span, spelling)?;
    item.token(import.semicolon_span, ";")?;
    item.finish()?;
    cursor.child(import.span)
}

fn source_facts(sources: &SourceMap, unit: &RawSourceUnit) -> Result<(), DeclarationError> {
    let file = sources.verify_file_id(unit.id).map_err(|_| arena::malformed())?;
    let source = sources.source(file).ok_or_else(arena::malformed)?;
    let end = u32::try_from(source.text().len()).map_err(|_| arena::malformed())?;
    let mut cursor = Cursor::new(sources, UntrustedSpan { file: unit.id, start: 0, end })?;
    let context = Context::authenticate(sources, unit)?;
    super::types::validate(sources, unit)?;
    for record in &unit.imports {
        import(sources, unit, &mut cursor, record, context)?;
    }
    let (mut data, mut functions) = (0, 0);
    while data < unit.data_declarations.len() || functions < unit.functions.len() {
        let declaration = unit.data_declarations.get(data);
        let function = unit.functions.get(functions);
        if declaration.is_some_and(|record| {
            function.is_none_or(|function| record.span.start < function.span.start)
        }) {
            let record = declaration.ok_or_else(arena::malformed)?;
            coverage::data(sources, unit, record, context)?;
            cursor.child(record.span)?;
            data += 1;
        } else {
            let record = function.ok_or_else(arena::malformed)?;
            coverage::function(sources, unit, record, context)?;
            super::body::validate(sources, unit, &record.body, context.function())?;
            cursor.child(record.span)?;
            functions += 1;
        }
    }
    cursor.finish()?;
    super::type_ownership::validate(sources, unit, context)
}

/// Authenticates complete v5 syntax without constructing semantic, IR or target authority.
///
/// # Errors
/// Rejects malformed/foreign/noncanonical source and arena claims project-wide before declaration
/// shape failures. Syntax budgets terminate without a partial seal. Providers remain unimplemented.
pub fn verify_snapshot(
    raw: RawProjectSyntaxSnapshot,
    sources: &SourceMap,
) -> Result<VerifiedProjectSyntaxV5, Vec<DeclarationError>> {
    if raw.schema_version != super::PROTOCOL_VERSION || raw.files.len() != sources.len() {
        return Err(vec![arena::malformed()]);
    }
    for (index, unit) in raw.files.iter().enumerate() {
        let Ok(file) = sources.verify_file_id(unit.id) else {
            return Err(vec![arena::malformed()]);
        };
        if unit.id as usize != index
            || sources.source(file).is_none_or(|source| source.path().as_str() != unit.path)
        {
            return Err(vec![arena::malformed()]);
        }
    }
    super::resources::preflight(sources, &raw).map_err(|error| vec![error])?;
    let mut errors = Errors::default();
    if let Err(error) = super::rejections::validate_advisories(sources, &raw) {
        errors.push(error);
    }
    for unit in &raw.files {
        if let Err(error) = source_facts(sources, unit) {
            errors.push(error);
        }
        if errors.terminal() {
            break;
        }
    }
    let errors = errors.finish();
    if !errors.is_empty() {
        return Err(errors);
    }
    // This call must remain below the complete project-wide raw/source/arena barrier.
    super::validate_declarations(sources, &raw).map_err(|error| vec![error])?;
    Ok(VerifiedProjectSyntaxV5::admitted(raw, sources, CompleteSyntaxProof(())))
}
