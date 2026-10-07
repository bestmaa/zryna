use std::mem::size_of;

use zryna_syntax::v5::RawExpressionKind;

use super::super::{DeclarationContext, DeclarationIdentity, DeclarationKind};
use super::BodyTypeFailure;
use super::model::{ArmRecord, FunctionRecords, Origin, ResultRecord, SourceRecord, Tables};

/// These are observations of actual allocated records, not language admission limits.
#[derive(Clone, Copy, Debug, Default)]
pub struct StorageReport {
    /// Actual source occurrence records.
    pub source_records: usize,
    /// Actual original expression slots.
    pub expression_records: usize,
    /// Actual original statement slots.
    pub statement_records: usize,
    /// Original arms, including arms which need no synthetic borrow head.
    pub arm_records: usize,
    /// Substitution environments actually issued while checking bodies.
    pub environments: usize,
    /// Capacity bytes of the persisted source/body tables, including embedded descriptors.
    pub table_capacity_bytes: usize,
    /// Actual source capability predicate rows, separate from body views.
    pub predicate_rows: usize,
    /// Actual source child/nominal/payload dependency edges.
    pub predicate_edges: usize,
    /// Capacity bytes of the temporary predicate graph, including supports and reverse edges.
    pub predicate_capacity_bytes: usize,
}

pub(super) fn reserve<T>(count: usize) -> Result<Vec<T>, BodyTypeFailure> {
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| BodyTypeFailure::AllocationFailure)?;
    Ok(values)
}

pub(super) fn repeated<T: Clone>(count: usize, value: T) -> Result<Vec<T>, BodyTypeFailure> {
    let mut values = reserve(count)?;
    values.resize(count, value);
    Ok(values)
}

pub(super) fn checked_add(left: usize, right: usize) -> Result<usize, BodyTypeFailure> {
    left.checked_add(right).ok_or(BodyTypeFailure::InternalFailure)
}

pub(super) fn tables(context: &DeclarationContext<'_>) -> Result<Tables, BodyTypeFailure> {
    // The borrowed authority has already checked every inherited protocol/declaration limit.
    // Recount from that authority; no maximum-sized universe is allocated.
    let mut sources = reserve(context.syntax().files().len())?;
    let count = context
        .syntax()
        .files()
        .iter()
        .try_fold(0, |count, unit| checked_add(count, unit.functions.len()))?;
    let mut functions = reserve(count)?;
    let mut function_offsets = reserve(context.syntax().files().len())?;
    for module in context.modules() {
        let unit = &context.syntax().files()[module.identity().index() as usize];
        let mut nodes = reserve(unit.type_syntax.len())?;
        let mut data = module.data_declarations().peekable();
        let mut signatures = module.functions().peekable();
        for node in &unit.type_syntax {
            while data.peek().is_some_and(|declaration| declaration.span().end() < node.span.start)
            {
                data.next();
            }
            while signatures
                .peek()
                .is_some_and(|declaration| declaration.span().end() < node.span.start)
            {
                signatures.next();
            }
            let owner = data
                .peek()
                .into_iter()
                .chain(signatures.peek())
                .find(|declaration| {
                    let span = declaration.span();
                    span.start() <= node.span.start && node.span.end <= span.end()
                })
                .ok_or(BodyTypeFailure::InternalFailure)?
                .identity();
            nodes.push(SourceRecord {
                owner,
                head: None,
                invalid_arguments: false,
                argument_occurrence: false,
            });
        }
        sources.push(nodes);
        function_offsets.push(functions.len());
        for declaration in module.functions() {
            let owner = declaration.identity();
            let body = &unit.functions[owner.source_index() as usize].body;
            let mut expressions = repeated(body.expressions.len(), ResultRecord::default())?;
            let arm_count = body.expressions.iter().try_fold(0, |count, expression| {
                let count_here = match &expression.kind {
                    RawExpressionKind::Match { arms, .. } => arms.len(),
                    _ => 0,
                };
                checked_add(count, count_here)
            })?;
            if arm_count > body.expressions.len() {
                return Err(BodyTypeFailure::InternalFailure);
            }
            let mut arms = reserve(arm_count)?;
            for (index, expression) in body.expressions.iter().enumerate() {
                expressions[index].arm_start = arms.len();
                if let RawExpressionKind::Match { arms: original, .. } = &expression.kind {
                    for arm in 0..original.len() {
                        arms.push(ArmRecord {
                            origin: Origin::ArmBorrow {
                                function: owner,
                                expression: u32::try_from(index)
                                    .map_err(|_| BodyTypeFailure::InternalFailure)?,
                                arm: u32::try_from(arm)
                                    .map_err(|_| BodyTypeFailure::InternalFailure)?,
                            },
                            payload: None,
                            head: None,
                            rank: 0,
                        });
                    }
                }
            }
            functions.push(FunctionRecords {
                owner,
                expressions,
                statements: repeated(body.statements.len(), ResultRecord::default())?,
                arms,
                environments: reserve(checked_add(body.expressions.len(), body.statements.len())?)?,
                next_rank: 0,
            });
        }
    }
    Ok(Tables { sources, functions, function_offsets })
}

pub(super) fn report(tables: &Tables) -> Result<StorageReport, BodyTypeFailure> {
    let mut result = StorageReport::default();
    let mut bytes = tables.sources.capacity() * size_of::<Vec<SourceRecord>>()
        + tables.functions.capacity() * size_of::<FunctionRecords>()
        + tables.function_offsets.capacity() * size_of::<usize>();
    for records in &tables.sources {
        result.source_records = checked_add(result.source_records, records.len())?;
        bytes = checked_add(bytes, records.capacity() * size_of::<SourceRecord>())?;
    }
    for function in &tables.functions {
        result.expression_records =
            checked_add(result.expression_records, function.expressions.len())?;
        result.statement_records =
            checked_add(result.statement_records, function.statements.len())?;
        result.arm_records = checked_add(result.arm_records, function.arms.len())?;
        result.environments = checked_add(result.environments, function.environments.len())?;
        bytes = checked_add(bytes, function.expressions.capacity() * size_of::<ResultRecord>())?;
        bytes = checked_add(bytes, function.statements.capacity() * size_of::<ResultRecord>())?;
        bytes = checked_add(bytes, function.arms.capacity() * size_of::<ArmRecord>())?;
        bytes = checked_add(
            bytes,
            function.environments.capacity() * size_of::<super::model::Environment>(),
        )?;
    }
    result.table_capacity_bytes = bytes;
    Ok(result)
}

pub(super) fn raw_function<'a>(
    context: &'a DeclarationContext<'_>,
    owner: DeclarationIdentity,
) -> &'a zryna_syntax::v5::RawFunctionSyntax {
    assert_eq!(owner.kind(), DeclarationKind::Function);
    &context.syntax().files()[owner.module().index() as usize].functions
        [owner.source_index() as usize]
}

/// Proof metadata only: tuple indices are used by the checker, never Cartesian allocations.
#[cfg(test)]
pub(super) fn comparison_domain(
    source_types: u128,
    parameters: u128,
    expressions: u128,
    statements: u128,
    arms: u128,
) -> Result<(u128, u128, u128), BodyTypeFailure> {
    let origins = source_types
        .checked_add(parameters)
        .and_then(|value| value.checked_add(expressions))
        .and_then(|value| value.checked_add(statements))
        .and_then(|value| value.checked_add(arms))
        .and_then(|value| value.checked_add(4))
        .ok_or(BodyTypeFailure::InternalFailure)?;
    let environments = expressions
        .checked_add(statements)
        .and_then(|value| value.checked_add(1))
        .ok_or(BodyTypeFailure::InternalFailure)?;
    let views = origins.checked_mul(environments).ok_or(BodyTypeFailure::InternalFailure)?;
    let pairs = views.checked_mul(views).ok_or(BodyTypeFailure::InternalFailure)?;
    let depth = 258_u128
        .checked_mul(environments)
        .and_then(|value| arms.checked_mul(2).and_then(|arms| value.checked_add(arms)))
        .ok_or(BodyTypeFailure::InternalFailure)?;
    Ok((views, pairs, depth))
}
