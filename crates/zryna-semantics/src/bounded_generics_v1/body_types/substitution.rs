use super::super::DeclarationIdentity;
use super::model::{Environment, Head, Kind, Origin, Tables, Ty};
use super::{BodyTypeFailure, Checker};

/// Environments are issued once at a source operation. Arguments are explicit source views,
/// or older nominal argument views, and never inferred expression results.
pub(super) fn environment(
    checker: &mut Checker<'_, '_>,
    function: DeclarationIdentity,
    owner: DeclarationIdentity,
    arguments: [Option<Ty>; 2],
) -> Result<u32, BodyTypeFailure> {
    if arguments[0].is_none() {
        return Ok(0);
    }
    let mut capabilities = [[super::capabilities::Status::False; 4]; 2];
    for (assume, outputs) in capabilities.iter_mut().enumerate() {
        for (index, argument) in arguments.into_iter().enumerate() {
            if let Some(argument) = argument {
                outputs[index * 2] = checker.capabilities.query(
                    &checker.tables,
                    function,
                    argument,
                    false,
                    assume != 0,
                )?;
                outputs[index * 2 + 1] = checker.capabilities.query(
                    &checker.tables,
                    function,
                    argument,
                    true,
                    assume != 0,
                )?;
            }
        }
    }
    let records = checker.tables.function_mut(function);
    let next = u32::try_from(records.environments.len() + 1)
        .map_err(|_| BodyTypeFailure::InternalFailure)?;
    if arguments.iter().flatten().any(|argument| argument.environment >= next) {
        return Err(BodyTypeFailure::InternalFailure);
    }
    if records.environments.len() == records.environments.capacity() {
        // The actual E+S reserve covers one environment per original issuing operation.
        return Err(BodyTypeFailure::InternalFailure);
    }
    records.environments.push(Environment { owner, arguments, capabilities });
    Ok(next)
}

pub(super) fn source(owner: DeclarationIdentity, occurrence: u32, environment: u32) -> Ty {
    Ty { origin: Origin::Source { module: owner.module().index(), occurrence }, environment }
}

/// Canonicalize parameter aliases only. Nominal members are never expanded here.
pub(super) fn normalize(
    tables: &Tables,
    function: DeclarationIdentity,
    mut ty: Ty,
) -> Result<Option<Ty>, BodyTypeFailure> {
    loop {
        let Origin::Source { module, occurrence } = ty.origin else {
            return Ok(Some(ty));
        };
        let Some(head) = tables.sources[module as usize][occurrence as usize].head else {
            return Ok(None);
        };
        let Kind::Parameter(parameter) = head.kind else {
            return Ok(Some(ty));
        };
        if ty.environment == 0 {
            return Ok(Some(ty));
        }
        let environment = tables
            .function(function)
            .environments
            .get(ty.environment as usize - 1)
            .ok_or(BodyTypeFailure::InternalFailure)?;
        if parameter.declaration() != environment.owner {
            return Ok(Some(ty));
        }
        let argument = environment.arguments[parameter.index() as usize]
            .ok_or(BodyTypeFailure::InternalFailure)?;
        if argument.environment >= ty.environment {
            return Err(BodyTypeFailure::InternalFailure);
        }
        ty = argument;
    }
}

pub(super) fn head(
    tables: &Tables,
    function: DeclarationIdentity,
    mut ty: Ty,
) -> Result<Option<Head>, BodyTypeFailure> {
    loop {
        let mut head = match ty.origin {
            Origin::Scalar(scalar) => Head::leaf(Kind::Scalar(scalar)),
            Origin::Source { module, occurrence } => {
                let Some(head) = tables.sources[module as usize][occurrence as usize].head else {
                    return Ok(None);
                };
                head
            }
            Origin::Expression { function: owner, expression } => {
                if owner != function || ty.environment != 0 {
                    return Err(BodyTypeFailure::InternalFailure);
                }
                let Some(head) = tables.function(owner).expressions[expression as usize].head
                else {
                    return Ok(None);
                };
                head
            }
            Origin::Statement { function: owner, statement } => {
                if owner != function || ty.environment != 0 {
                    return Err(BodyTypeFailure::InternalFailure);
                }
                let Some(head) = tables.function(owner).statements[statement as usize].head else {
                    return Ok(None);
                };
                head
            }
            origin @ Origin::ArmBorrow { function: owner, expression, arm } => {
                if owner != function || ty.environment != 0 {
                    return Err(BodyTypeFailure::InternalFailure);
                }
                let records = tables.function(owner);
                let record = records
                    .arms
                    .get(records.expressions[expression as usize].arm_start + arm as usize)
                    .ok_or(BodyTypeFailure::InternalFailure)?;
                if record.origin != origin {
                    return Err(BodyTypeFailure::InternalFailure);
                }
                let Some(head) = record.head else { return Ok(None) };
                head
            }
        };
        if ty.environment != 0 {
            let environment = tables
                .function(function)
                .environments
                .get(ty.environment as usize - 1)
                .ok_or(BodyTypeFailure::InternalFailure)?;
            if let Kind::Parameter(parameter) = head.kind
                && parameter.declaration() == environment.owner
            {
                let Some(argument) = environment.arguments[parameter.index() as usize] else {
                    return Err(BodyTypeFailure::InternalFailure);
                };
                if argument.environment >= ty.environment {
                    return Err(BodyTypeFailure::InternalFailure);
                }
                ty = argument;
                continue;
            }
            for child in head.children.iter_mut().flatten() {
                if !matches!(child.origin, Origin::Source { .. }) {
                    return Err(BodyTypeFailure::InternalFailure);
                }
                child.environment = ty.environment;
            }
        }
        return Ok(Some(head));
    }
}

pub(super) fn issue_expression(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    expression: u32,
    head: Head,
) -> Result<Ty, BodyTypeFailure> {
    validate_children(&checker.tables, owner, head)?;
    let records = checker.tables.function_mut(owner);
    records.next_rank = records.next_rank.checked_add(1).ok_or(BodyTypeFailure::InternalFailure)?;
    let record = &mut records.expressions[expression as usize];
    record.head = Some(head);
    record.rank = records.next_rank;
    Ok(Ty { origin: Origin::Expression { function: owner, expression }, environment: 0 })
}

pub(super) fn validate_children(
    tables: &Tables,
    owner: DeclarationIdentity,
    head: Head,
) -> Result<(), BodyTypeFailure> {
    for child in head.children.into_iter().flatten() {
        if head_rank(tables, owner, child)? > tables.function(owner).next_rank {
            return Err(BodyTypeFailure::InternalFailure);
        }
    }
    Ok(())
}

pub(super) fn head_rank(
    tables: &Tables,
    owner: DeclarationIdentity,
    ty: Ty,
) -> Result<u32, BodyTypeFailure> {
    let records = tables.function(owner);
    Ok(match ty.origin {
        Origin::Expression { function, expression } if function == owner => {
            records.expressions[expression as usize].rank
        }
        Origin::Statement { function, statement } if function == owner => {
            records.statements[statement as usize].rank
        }
        Origin::ArmBorrow { function, expression, arm } if function == owner => {
            records.arms[records.expressions[expression as usize].arm_start + arm as usize].rank
        }
        Origin::Source { .. } | Origin::Scalar(_) => 0,
        _ => return Err(BodyTypeFailure::InternalFailure),
    })
}
