//! Structured no-phi branches: original opaque states must agree without owner repair.
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
                // No phi is issued by this slice. An incoming lexical binding must keep its
                // exact place identity, even if both arms replace it with the same type.
                if self.locals.iter().zip(&saved).any(|(a, b)| a.value.id != b.value.id) {
                    return Err(self.locate(
                        super::owners::ownership(
                            "branch replacement requires a separate phi proof",
                        ),
                        span,
                    ));
                }
                self.charge_branch_state()?;
                outputs.push((
                    self.block,
                    self.alive[..saved_alive.len()].to_vec(),
                    self.loans.clone(),
                    self.loan_parents.clone(),
                ));
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
        let Some((_, alive, loans, parents)) = outputs.first() else {
            // Both arms return directly. There is no unreachable join block or return phi.
            return Ok(true);
        };
        if outputs.iter().any(|(_, a, l, p)| a != alive || l != loans || p != parents) {
            return Err(self.locate(
                super::owners::ownership("owner/loan state differs at structured branch join"),
                span,
            ));
        }
        self.charge_branch_state()?;
        self.alive.clone_from(alive);
        self.alive.resize(self.next as usize, false);
        self.loans.clone_from(loans);
        self.loan_parents.clone_from(parents);
        self.block = self.new_block(span)?;
        for (block, ..) in outputs {
            self.blocks[block].terminator = raw::Terminator::Jump(raw::Edge {
                target: u32::try_from(self.block).map_err(|_| Failure::InternalFailure)?,
                arguments: Vec::new(),
            });
        }
        Ok(false)
    }

    fn charge_branch_state(&mut self) -> Result<(), Failure> {
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
