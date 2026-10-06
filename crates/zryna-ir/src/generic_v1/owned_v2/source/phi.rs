//! Sparse finite concrete `Copy` replacement joins; no owned or opaque place is repaired.
use super::{Binding, Builder, Closed, Failure, Value, raw, reserve};
use std::collections::BTreeMap;
use zryna_source::UntrustedSpan;

const OPTION_I32: &[u8] = &[0x14, 1, 0, 0, 0, 1, 0, 0, 0, 1];
const RESULT_I32_BOOL: &[u8] = &[0x15, 2, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0];

fn copy_key(ty: &Closed) -> Option<&[u8]> {
    match ty {
        Closed::Stored(key)
            if matches!(key.as_slice(), [0 | 1])
                || key.as_slice() == OPTION_I32
                || key.as_slice() == RESULT_I32_BOOL =>
        {
            Some(key)
        }
        _ => None,
    }
}

fn parameter_ceiling(composite: bool) -> Failure {
    crate::generic_v1::budget(if composite {
        "finite Copy branch join parameter ceiling"
    } else {
        "scalar branch join parameter ceiling"
    })
}

pub(super) struct Output {
    pub block: usize,
    pub alive: Vec<bool>,
    pub loans: BTreeMap<u32, (u32, bool)>,
    pub parents: BTreeMap<u32, u32>,
    pub changes: Vec<(usize, Value)>,
}

impl Builder<'_, '_> {
    pub(super) fn copy_changes(
        &mut self,
        saved: &[Binding<'_>],
        span: UntrustedSpan,
    ) -> Result<Vec<(usize, Value)>, Failure> {
        if self.locals.len() != saved.len() {
            return Err(Failure::InternalFailure);
        }
        let mut count = 0usize;
        let mut units = 0usize;
        let mut composite = false;
        for (current, prior) in self.locals.iter().zip(saved) {
            if current.value.id == prior.value.id {
                continue;
            }
            if !prior.mutable
                || copy_key(&prior.value.ty).is_none()
                || current.value.ty != prior.value.ty
                || current.name != prior.name
                || current.mutable != prior.mutable
                || !self.alive[current.value.id as usize]
                || self.affine[current.value.id as usize]
                || self.affine[prior.value.id as usize]
                || self
                    .loans
                    .values()
                    .any(|(root, _)| *root == prior.value.id || *root == current.value.id)
            {
                return Err(self.locate(
                    super::owners::ownership(
                        "branch replacement requires a separate owner phi proof",
                    ),
                    span,
                ));
            }
            count = count.checked_add(1).ok_or(Failure::InternalFailure)?;
            let key = copy_key(&prior.value.ty).ok_or(Failure::InternalFailure)?;
            composite |= key.len() > 1;
            units = units
                .checked_add(3)
                .and_then(|used| used.checked_add(key.len()))
                .ok_or_else(|| crate::generic_v1::budget("Copy branch capture credit overflow"))?;
        }
        if count > crate::data_ownership_v1::MAX_BLOCK_PARAMETERS {
            return Err(parameter_ceiling(composite));
        }
        // Each sparse entry retains an index, ID, descriptor and its complete original key.
        // Charge every byte before the reserve or Value clone; scalar keys still cost four.
        self.budget.branch_state(units)?;
        let mut changes = reserve(count)?;
        for (index, (current, prior)) in self.locals.iter().zip(saved).enumerate() {
            if current.value.id != prior.value.id {
                changes.push((index, current.value.clone()));
            }
        }
        Ok(changes)
    }

    pub(super) fn copy_join(&mut self, outputs: &[Output]) -> Result<(), Failure> {
        let changed = |index| {
            outputs.iter().any(|o| o.changes.binary_search_by_key(&index, |(i, _)| *i).is_ok())
        };
        let count = (0..self.locals.len()).filter(|&index| changed(index)).count();
        if count > crate::data_ownership_v1::MAX_BLOCK_PARAMETERS {
            let composite = (0..self.locals.len()).any(|index| {
                changed(index)
                    && copy_key(&self.locals[index].value.ty).is_some_and(|key| key.len() > 1)
            });
            return Err(parameter_ceiling(composite));
        }
        if count == 0 {
            return Ok(());
        }
        // Index, fresh parameter ID, both actual key copies and each incoming edge operand.
        // Parameter and value arena ceilings remain independently enforced.
        let units = (0..self.locals.len()).filter(|&index| changed(index)).try_fold(
            0usize,
            |used, index| {
                let key = copy_key(&self.locals[index].value.ty).ok_or(Failure::InternalFailure)?;
                key.len()
                    .checked_mul(2)
                    .and_then(|keys| keys.checked_add(2))
                    .and_then(|base| base.checked_add(outputs.len()))
                    .and_then(|entry| used.checked_add(entry))
                    .ok_or_else(|| crate::generic_v1::budget("Copy branch join credit overflow"))
            },
        )?;
        self.budget.branch_state(units)?;
        let mut indexes = reserve(count)?;
        indexes.extend((0..self.locals.len()).filter(|&index| changed(index)));
        self.blocks[self.block].parameters = reserve(count)?;
        for output in outputs {
            let mut arguments = reserve(count)?;
            for &index in &indexes {
                let id = output
                    .changes
                    .binary_search_by_key(&index, |(i, _)| *i)
                    .map_or(self.locals[index].value.id, |i| output.changes[i].1.id);
                arguments.push(id);
            }
            let raw::Terminator::Jump(edge) = &mut self.blocks[output.block].terminator else {
                return Err(Failure::InternalFailure);
            };
            edge.arguments = arguments;
        }
        for index in indexes {
            let value = self.value(self.locals[index].value.ty.clone())?;
            let definition = self.definition(&value)?;
            self.blocks[self.block].parameters.push(definition);
            self.locals[index].value = value;
        }
        Ok(())
    }
}
