use super::{
    BTreeSet, Errors, MAX_NESTING_DEPTH, NormalizedSourcePath, RawSourceUnit, SourceMap,
    SourceUnit, ordered, verify_declaration, verify_file_structure, verify_function, verify_import,
    verify_type_arena,
};

pub(in crate::v4) fn verify_file(
    raw: RawSourceUnit,
    position: usize,
    sources: &SourceMap,
    errors: &mut Errors,
) -> Option<SourceUnit> {
    let expected = u32::try_from(position).ok()?;
    if raw.id != expected {
        errors.protocol(None, "source units are not in canonical dense file-id order");
        return None;
    }
    let id = sources.verify_file_id(raw.id).ok()?;
    let path = match NormalizedSourcePath::new(raw.path.clone()) {
        Ok(path) => path,
        Err(_) => {
            errors.protocol(None, "source unit path is not portable and normalized");
            return None;
        }
    };
    if sources.source(id).is_none_or(|source| source.path() != &path) {
        errors
            .protocol(Some(path.as_str()), "source path does not match the authoritative file id");
        return None;
    }
    let mut import_end = 0;
    for import in &raw.imports {
        verify_import(import, raw.id, &path, sources, errors);
        ordered(import.span, &mut import_end, &path, errors, "import");
    }
    let mut type_owners = vec![0u32; raw.type_syntax.len()];
    let type_depths =
        verify_type_arena(&raw.type_syntax, raw.id, &path, sources, &mut type_owners, errors);
    let mut top_names = BTreeSet::new();
    let mut declaration_end = 0;
    let mut function_end = 0;
    let mut top_spans =
        Vec::with_capacity(raw.data_declarations.len().saturating_add(raw.functions.len()));
    for declaration in &raw.data_declarations {
        verify_declaration(
            declaration,
            raw.id,
            &path,
            sources,
            &mut type_owners,
            &mut top_names,
            errors,
        );
        ordered(declaration.span, &mut declaration_end, &path, errors, "data declaration");
        top_spans.push(declaration.span);
    }
    for function in &raw.functions {
        verify_function(function, raw.id, &path, sources, &mut type_owners, &mut top_names, errors);
        ordered(function.span, &mut function_end, &path, errors, "function");
        top_spans.push(function.span);
    }
    top_spans.sort_by_key(|span| (span.start, span.end));
    let mut merged_end = import_end;
    for span in top_spans {
        if span.start < merged_end {
            errors.node(&path, "top-level declaration overlaps an import or another declaration");
        }
        merged_end = merged_end.max(span.end);
    }
    if type_depths.iter().any(|depth| *depth > MAX_NESTING_DEPTH) {
        errors.limit("type syntax nesting exceeds the protocol-v4 limit");
    }
    if type_owners.iter().any(|owners| *owners != 1) {
        errors.node(&path, "type arena has a shared or orphan node");
    }
    verify_file_structure(&raw, &path, errors);
    Some(SourceUnit { id, path, raw })
}
