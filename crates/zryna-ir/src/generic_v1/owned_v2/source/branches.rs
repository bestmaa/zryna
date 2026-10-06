//! Structured scalar joins: original opaque ownership must agree without owner repair.
use super::phi::Output;
use super::{Builder, Closed, Failure, raw, reject, reserve};
use zryna_source::UntrustedSpan;

impl Builder<'_, '_> {
    pub(super) fn branch(
        &mut self,
        condition: u32,
        yes: u32,
        no: Option<u32>,
        span: UntrustedSpan,
        depth: usize,
    ) -> Result<bool, Failure> {
        let condition = self.expression(condition, depth + 1)?;
        if condition.ty != Closed::Stored(vec![0]) {
            return Err(reject("structured branch requires an exact bool condition"));
        }
        let entry = self.block;
        self.charge_branch_state()?;
        let saved = self.locals.clone();
        let saved_alive = self.alive.clone();
        let saved_loans = self.loans.clone();
        let saved_parents = self.loan_parents.clone();
        let prior_scope = self.scope_start;
        let mut edges = reserve(2)?;
        let mut outputs = reserve(2)?;
        for source in [Some(yes), no] {
            self.charge_branch_state()?;
            self.locals = saved.clone();
            self.scope_start = prior_scope;
            self.alive.clone_from(&saved_alive);
            self.alive.resize(self.next as usize, false);
            self.loans = saved_loans.clone();
            self.loan_parents = saved_parents.clone();
            self.block = self.new_block(span)?;
            edges.push(raw::Edge {
                target: u32::try_from(self.block).map_err(|_| Failure::InternalFailure)?,
                arguments: Vec::new(),
            });
            let returned = if let Some(source) = source {
                self.scoped_block(source, span, depth + 1)?
            } else {
                false
            };
            if !returned {
                let changes = self.scalar_changes(&saved, span)?;
                self.charge_branch_state()?;
                outputs.push(Output {
                    block: self.block,
                    alive: self.alive[..saved_alive.len()].to_vec(),
                    loans: self.loans.clone(),
                    parents: self.loan_parents.clone(),
                    changes,
                });
            }
        }
        self.blocks[entry].span = span;
        self.blocks[entry].terminator = raw::Terminator::Branch {
            condition: condition.id,
            yes: edges.remove(0),
            no: edges.remove(0),
        };
        self.locals = saved;
        self.scope_start = prior_scope;
        let Some(first) = outputs.first() else {
            // Both arms return directly. There is no unreachable join block or return phi.
            return Ok(true);
        };
        if outputs
            .iter()
            .any(|o| o.alive != first.alive || o.loans != first.loans || o.parents != first.parents)
        {
            return Err(self.locate(
                super::owners::ownership("owner/loan state differs at structured branch join"),
                span,
            ));
        }
        self.charge_branch_state()?;
        self.alive.clone_from(&first.alive);
        self.alive.resize(self.next as usize, false);
        self.loans.clone_from(&first.loans);
        self.loan_parents.clone_from(&first.parents);
        self.block = self.new_block(span)?;
        for output in &outputs {
            self.blocks[output.block].terminator = raw::Terminator::Jump(raw::Edge {
                target: u32::try_from(self.block).map_err(|_| Failure::InternalFailure)?,
                arguments: Vec::new(),
            });
        }
        self.scalar_join(&outputs)?;
        Ok(false)
    }

    pub(super) fn charge_branch_state(&mut self) -> Result<(), Failure> {
        // Charge complete snapshots/restores, including Copy availability and lexical places,
        // before allocating. The same credit persists across every closed specialization.
        let units =
            [self.locals.len(), self.next as usize, self.loans.len(), self.loan_parents.len(), 1]
                .into_iter()
                .try_fold(0usize, usize::checked_add)
                .ok_or_else(|| {
                    crate::generic_v1::budget("structured branch state credit overflow")
                })?;
        // Binding snapshots also clone their complete type keys. A deeply nested family
        // cannot hide copied bytes behind one lexical-place credit.
        let units = self
            .locals
            .iter()
            .try_fold(units, |used, binding| {
                let bytes = match &binding.value.ty {
                    Closed::Stored(key) | Closed::Borrow(key, _) => key.len(),
                    Closed::Unit => 0,
                };
                used.checked_add(bytes)
            })
            .ok_or_else(|| {
                crate::generic_v1::budget("structured branch type-key credit overflow")
            })?;
        self.budget.branch_state(units)
    }
}
