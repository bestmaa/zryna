//! Sparse concrete scalar replacement joins; no owned or opaque place is repaired.
use super::{Binding, Builder, Closed, Failure, Value, raw, reserve};
use std::collections::BTreeMap;
use zryna_source::UntrustedSpan;

pub(super) struct Output {
    pub block: usize,
    pub alive: Vec<bool>,
    pub loans: BTreeMap<u32, (u32, bool)>,
    pub parents: BTreeMap<u32, u32>,
    pub changes: Vec<(usize, Value)>,
}

impl Builder<'_, '_> {
    pub(super) fn scalar_changes(
        &mut self,
        saved: &[Binding<'_>],
        span: UntrustedSpan,
    ) -> Result<Vec<(usize, Value)>, Failure> {
        if self.locals.len() != saved.len() {
            return Err(Failure::InternalFailure);
        }
        let mut count = 0usize;
        for (current, prior) in self.locals.iter().zip(saved) {
            if current.value.id == prior.value.id {
                continue;
            }
            if !prior.mutable
                || !matches!(&prior.value.ty, Closed::Stored(key) if matches!(key.as_slice(), [0 | 1]))
                || current.value.ty != prior.value.ty
                || current.name != prior.name
                || current.mutable != prior.mutable
                || !self.alive[current.value.id as usize]
                || self.affine[current.value.id as usize]
            {
                return Err(self.locate(
                    super::owners::ownership(
                        "branch replacement requires a separate owner phi proof",
                    ),
                    span,
                ));
            }
            count = count.checked_add(1).ok_or(Failure::InternalFailure)?;
        }
        if count > crate::data_ownership_v1::MAX_BLOCK_PARAMETERS {
            return Err(crate::generic_v1::budget("scalar branch join parameter ceiling"));
        }
        // Each sparse entry retains a place index, ID, type descriptor and one copied key byte.
        self.budget.branch_state(count.checked_mul(4).ok_or(Failure::InternalFailure)?)?;
        let mut changes = reserve(count)?;
        for (index, (current, prior)) in self.locals.iter().zip(saved).enumerate() {
            if current.value.id != prior.value.id {
                changes.push((index, current.value.clone()));
            }
        }
        Ok(changes)
    }

    pub(super) fn scalar_join(&mut self, outputs: &[Output]) -> Result<(), Failure> {
        let changed = |index| {
            outputs.iter().any(|o| o.changes.binary_search_by_key(&index, |(i, _)| *i).is_ok())
        };
        let count = (0..self.locals.len()).filter(|&index| changed(index)).count();
        if count > crate::data_ownership_v1::MAX_BLOCK_PARAMETERS {
            return Err(crate::generic_v1::budget("scalar branch join parameter ceiling"));
        }
        if count == 0 {
            return Ok(());
        }
        // Index, fresh parameter ID, both actual key copies and each incoming edge operand.
        // Parameter and value arena ceilings remain independently enforced.
        let units = count
            .checked_mul(outputs.len().checked_add(4).ok_or(Failure::InternalFailure)?)
            .ok_or(Failure::InternalFailure)?;
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
