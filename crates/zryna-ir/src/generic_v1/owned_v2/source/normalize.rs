//! Canonical dense result order after edge-specific cleanup insertion.
use super::{Extension, Failure, Owned, raw, reserve};
pub(super) fn run(blocks: &mut [raw::Block], extensions: &mut [Extension]) -> Result<(), Failure> {
    let count = blocks.iter().map(|b| b.parameters.len() + b.instructions.len()).sum::<usize>();
    let mut ids = reserve(count)?;
    ids.resize(count, u32::MAX);
    let mut next = 0u32;
    for b in blocks.iter() {
        for id in
            b.parameters.iter().map(|p| p.id).chain(b.instructions.iter().map(|i| i.result.id))
        {
            let slot = ids.get_mut(id as usize).ok_or(Failure::InternalFailure)?;
            if *slot != u32::MAX {
                return Err(Failure::InternalFailure);
            }
            *slot = next;
            next += 1;
        }
    }
    let map = |id: &mut u32| -> Result<(), Failure> {
        *id = *ids.get(*id as usize).filter(|n| **n != u32::MAX).ok_or(Failure::InternalFailure)?;
        Ok(())
    };
    let edge = |e: &mut raw::Edge| -> Result<(), Failure> {
        for id in &mut e.arguments {
            map(id)?;
        }
        Ok(())
    };
    for b in blocks {
        for p in &mut b.parameters {
            map(&mut p.id)?;
        }
        for i in &mut b.instructions {
            map(&mut i.result.id)?;
            match &mut i.operation {
                raw::Operation::Copy { value } => map(value)?,
                raw::Operation::I32Add { left, right } => {
                    map(left)?;
                    map(right)?;
                }
                raw::Operation::ClosedGenericCall { arguments, .. }
                | raw::Operation::SourceCall { arguments, .. } => {
                    for id in arguments {
                        map(id)?;
                    }
                }
                raw::Operation::ClosedEnumConstruct { payload: Some(id), .. } => map(id)?,
                _ => {}
            }
        }
        match &mut b.terminator {
            raw::Terminator::Return(id) => map(id)?,
            raw::Terminator::Jump(e) => edge(e)?,
            raw::Terminator::Branch { condition, yes, no } => {
                map(condition)?;
                edge(yes)?;
                edge(no)?;
            }
            raw::Terminator::ClosedEnumMatch { scrutinee, arms, .. } => {
                map(scrutinee)?;
                for a in arms {
                    if let Some(b) = &mut a.binding {
                        map(&mut b.id)?;
                    }
                    edge(&mut a.edge)?;
                }
            }
        }
    }
    for e in extensions.iter_mut() {
        map(&mut e.result)?;
        match &mut e.operation {
            Owned::Move(id) | Owned::EndLoan(id) | Owned::Drop(id) | Owned::CloneString(id) => {
                map(id)?;
            }
            Owned::Borrow { value, .. } => map(value)?,
            Owned::StringLiteral(_) => {}
        }
    }
    extensions.sort_by_key(|e| e.result);
    Ok(())
}
