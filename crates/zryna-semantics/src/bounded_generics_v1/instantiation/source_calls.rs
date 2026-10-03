//! Original source-call cycles remain rejected even in unused opaque templates.

use super::diagnostics::Errors;
use super::model::Builder;
use super::{DeclarationIdentity, DeclarationKind, InstantiationFailure, push, reserve};
use zryna_source::UntrustedSpan;
use zryna_syntax::v5::RawExpressionKind;

type Edges = Vec<Vec<(usize, UntrustedSpan)>>;

pub(super) fn check(builder: &Builder<'_, '_, '_>) -> Result<(), InstantiationFailure> {
    let (owners, edges) = graph(builder)?;
    let mut states = reserve(owners.len())?;
    states.resize(owners.len(), 0u8);
    let mut stack = reserve(owners.len())?;
    let mut errors = Errors::default();
    for root in 0..owners.len() {
        if states[root] != 0 {
            continue;
        }
        states[root] = 1;
        stack.push((root, 0));
        while let Some((index, next)) = stack.last_mut() {
            if *next == edges[*index].len() {
                states[*index] = 2;
                stack.pop();
                continue;
            }
            let (child, span) = edges[*index][*next];
            *next += 1;
            match states[child] {
                1 => {
                    let witness = cycle_witness(&owners, &stack, child)?;
                    errors.at(
                        builder.bodies,
                        "ZRYNA-M7003",
                        Some(span),
                        &[],
                        &witness,
                        format!(
                            "source-level function recursion is excluded before specialization; original declaration path {witness:?}"
                        ),
                    )?;
                }
                0 => {
                    states[child] = 1;
                    push(&mut stack, (child, 0))?;
                }
                _ => {}
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(InstantiationFailure::Diagnostics(errors.finish()?))
    }
}

fn graph(
    builder: &Builder<'_, '_, '_>,
) -> Result<(Vec<DeclarationIdentity>, Edges), InstantiationFailure> {
    let declarations = builder.bodies.declarations();
    let mut owners = reserve(super::checked_count(
        declarations.syntax().files().iter().map(|file| file.functions.len()),
    )?)?;
    owners.extend(
        declarations
            .modules()
            .flat_map(super::super::ModuleView::functions)
            .map(super::super::DeclarationView::identity),
    );
    let mut edges = reserve(owners.len())?;
    edges.resize_with(owners.len(), Vec::new);
    for (index, owner) in owners.iter().enumerate() {
        let function = &declarations.syntax().files()[owner.module().index() as usize].functions
            [owner.source_index() as usize];
        for expression in &function.body.expressions {
            if let RawExpressionKind::Call { callee, .. } = &expression.kind
                && let Some(target) = builder.target(*owner, &callee.text)
                && target.kind() == DeclarationKind::Function
            {
                let to = owners
                    .binary_search(&target)
                    .map_err(|_| InstantiationFailure::InternalFailure)?;
                push(&mut edges[index], (to, callee.span))?;
            }
        }
        edges[index].sort_unstable_by_key(|(to, span)| (*to, span.start, span.end));
    }
    Ok((owners, edges))
}

fn cycle_witness(
    owners: &[DeclarationIdentity],
    stack: &[(usize, usize)],
    child: usize,
) -> Result<Vec<usize>, InstantiationFailure> {
    let start = stack
        .iter()
        .position(|(owner, _)| *owner == child)
        .ok_or(InstantiationFailure::InternalFailure)?;
    let count = stack
        .len()
        .checked_sub(start)
        .and_then(|n| n.checked_add(1))
        .and_then(|n| n.checked_mul(2))
        .ok_or(InstantiationFailure::InternalFailure)?;
    let mut witness = reserve(count)?;
    for index in stack[start..].iter().map(|(index, _)| *index).chain(std::iter::once(child)) {
        let owner = owners[index];
        witness.push(
            usize::try_from(owner.module().index())
                .map_err(|_| InstantiationFailure::InternalFailure)?,
        );
        witness.push(
            usize::try_from(owner.source_index())
                .map_err(|_| InstantiationFailure::InternalFailure)?,
        );
    }
    Ok(witness)
}
