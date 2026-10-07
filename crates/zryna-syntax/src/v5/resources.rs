use zryna_source::SourceMap;

use super::{DeclarationError, RawDataDeclarationKind, RawProjectSyntaxSnapshot};
use crate::v4;

fn limit(actual: usize, maximum: usize) -> Result<(), DeclarationError> {
    if actual > maximum {
        Err(DeclarationError { code: "ZRYNA-Y5201", span: None })
    } else {
        Ok(())
    }
}

fn add(total: &mut usize, amount: usize, maximum: usize) -> Result<(), DeclarationError> {
    *total =
        total.checked_add(amount).ok_or(DeclarationError { code: "ZRYNA-Y5201", span: None })?;
    limit(*total, maximum)
}

pub(super) fn preflight(
    sources: &SourceMap,
    raw: &RawProjectSyntaxSnapshot,
) -> Result<(), DeclarationError> {
    limit(raw.files.len(), zryna_source::MAX_SOURCE_FILES)?;
    limit(raw.diagnostics.len(), v4::MAX_PROVIDER_DIAGNOSTICS)?;
    let mut source_bytes = 0;
    let mut imports = 0;
    let mut bindings = 0;
    let mut types = 0;
    let mut declarations = 0;
    let mut members = 0;
    let mut functions = 0;
    let mut parameters = 0;
    let mut blocks = 0;
    let mut statements = 0;
    let mut expressions = 0;
    let mut locals = 0;
    let mut operands = 0;
    let mut arms = 0;
    for unit in &raw.files {
        let file =
            sources.verify_file_id(unit.id).map_err(|_| DeclarationError::malformed(None))?;
        let source = sources.source(file).ok_or_else(|| DeclarationError::malformed(None))?;
        add(&mut source_bytes, source.text().len(), v4::MAX_AGGREGATE_SOURCE_BYTES)?;
        limit(unit.imports.len(), v4::MAX_IMPORTS_PER_MODULE)?;
        add(&mut imports, unit.imports.len(), v4::MAX_IMPORTS_PER_PROJECT)?;
        for import in &unit.imports {
            limit(import.bindings.len(), v4::MAX_IMPORTED_NAMES_PER_DECLARATION)?;
            add(&mut bindings, import.bindings.len(), v4::MAX_IMPORTED_NAMES_PER_PROJECT)?;
        }
        limit(unit.type_syntax.len(), v4::MAX_TYPE_NODES_PER_MODULE)?;
        add(&mut types, unit.type_syntax.len(), v4::MAX_TYPE_NODES_PER_PROJECT)?;
        limit(unit.data_declarations.len(), v4::MAX_DATA_DECLARATIONS_PER_MODULE)?;
        add(
            &mut declarations,
            unit.data_declarations.len(),
            v4::MAX_DATA_DECLARATIONS_PER_PROJECT,
        )?;
        for data in &unit.data_declarations {
            let count = match &data.kind {
                RawDataDeclarationKind::Struct { fields, .. } => fields.len(),
                RawDataDeclarationKind::Enum { variants, .. } => variants.len(),
            };
            limit(count, v4::MAX_MEMBERS_PER_DECLARATION)?;
            add(&mut members, count, v4::MAX_MEMBERS_PER_PROJECT)?;
        }
        limit(unit.functions.len(), v4::MAX_FUNCTIONS_PER_MODULE)?;
        add(&mut functions, unit.functions.len(), v4::MAX_FUNCTIONS_PER_PROJECT)?;
        for function in &unit.functions {
            limit(function.parameters.len(), v4::MAX_PARAMETERS_PER_FUNCTION)?;
            add(&mut parameters, function.parameters.len(), v4::MAX_PARAMETERS_PER_PROJECT)?;
            limit(function.body.blocks.len(), v4::MAX_BLOCKS_PER_FUNCTION)?;
            add(&mut blocks, function.body.blocks.len(), v4::MAX_BLOCKS_PER_PROJECT)?;
            limit(function.body.statements.len(), v4::MAX_STATEMENTS_PER_FUNCTION)?;
            add(&mut statements, function.body.statements.len(), v4::MAX_STATEMENTS_PER_PROJECT)?;
            limit(function.body.expressions.len(), v4::MAX_EXPRESSIONS_PER_FUNCTION)?;
            add(
                &mut expressions,
                function.body.expressions.len(),
                v4::MAX_EXPRESSIONS_PER_PROJECT,
            )?;
            for block in &function.body.blocks {
                limit(block.statements.len(), v4::MAX_STATEMENTS_PER_FUNCTION)?;
            }
            let local_count = function
                .body
                .statements
                .iter()
                .filter(|statement| {
                    matches!(statement.kind, v4::RawStatementKind::LocalDeclaration { .. })
                })
                .count();
            limit(local_count, crate::v3::MAX_LOCALS_PER_FUNCTION)?;
            add(&mut locals, local_count, crate::v3::MAX_LOCALS_PER_PROJECT)?;
            for expression in &function.body.expressions {
                use super::RawExpressionKind as Kind;
                match &expression.kind {
                    Kind::Call { arguments, .. } => limit(arguments.len(), 256)?,
                    Kind::StructConstruction { fields, .. } => {
                        limit(fields.len(), v4::MAX_INITIALIZERS_PER_CONSTRUCTION)?;
                        add(&mut operands, fields.len(), v4::MAX_AGGREGATE_OPERANDS_PER_PROJECT)?;
                    }
                    Kind::VecConstruction { elements, .. }
                    | Kind::FixedArrayConstruction { elements, .. } => {
                        limit(elements.len(), v4::MAX_ELEMENTS_PER_CONSTRUCTION)?;
                        add(&mut operands, elements.len(), v4::MAX_AGGREGATE_OPERANDS_PER_PROJECT)?;
                    }
                    Kind::Match { arms: records, .. } => {
                        limit(records.len(), v4::MAX_MATCH_ARMS_PER_EXPRESSION)?;
                        add(&mut arms, records.len(), v4::MAX_MATCH_ARMS_PER_PROJECT)?;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
