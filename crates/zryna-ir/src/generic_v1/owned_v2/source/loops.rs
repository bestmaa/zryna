//! Repeated headers carry scalar Copy places; opaque ownership never receives a phi.
use super::{Builder, Closed, Failure, raw, reject, reserve};
use zryna_source::UntrustedSpan;

impl Builder<'_, '_> {
    pub(super) fn source_loop(
        &mut self,
        condition: u32,
        body: u32,
        span: UntrustedSpan,
        depth: usize,
    ) -> Result<(), Failure> {
        let (carried, arguments) = self.loop_header_arguments(span)?;
        let count = carried.len();
        let preheader = self.block;
        self.block = self.new_block(span)?;
        let header = self.block;
        let header_id = u32::try_from(header).map_err(|_| Failure::InternalFailure)?;
        self.blocks[preheader].terminator =
            raw::Terminator::Jump(raw::Edge { target: header_id, arguments });
        self.blocks[header].parameters = reserve(count)?;
        for &index in &carried {
            let value = self.value(self.locals[index].value.ty.clone())?;
            let definition = self.definition(&value)?;
            self.blocks[header].parameters.push(definition);
            self.locals[index].value = value;
        }
        self.charge_branch_state()?;
        let saved = self.locals.clone();
        let alive = self.alive.clone();
        let loans = self.loans.clone();
        let parents = self.loan_parents.clone();
        let prior_scope = self.scope_start;
        let start = self.next as usize;
        // Condition operations belong to the header, including temporary cleanup: they
        // execute again on every backedge and on the final false visit.
        let condition = self.expression(condition, depth + 1)?;
        if condition.ty != Closed::Stored(vec![0]) {
            return Err(reject("structured loop requires an exact bool condition"));
        }
        self.close_scope(start, None, span)?;
        let condition_exit = self.block;
        if self.alive[..alive.len()] != alive || self.loans != loans || self.loan_parents != parents
        {
            return Err(self.locate(
                super::owners::ownership("condition changes incoming loop owner/loan state"),
                span,
            ));
        }
        let body_id = self.new_block(span)?;
        self.block = body_id;
        let returned = self.scoped_block(body, span, depth + 1)?;
        if !returned {
            if self.locals.iter().zip(&saved).enumerate().any(|(i, (a, b))| {
                a.value.ty != b.value.ty || (!carried.contains(&i) && a.value.id != b.value.id)
            }) || self.alive[..alive.len()] != alive
                || self.loans != loans
                || self.loan_parents != parents
            {
                return Err(self.locate(
                    super::owners::ownership("owner/loan state differs at loop backedge"),
                    span,
                ));
            }
            let mut arguments = reserve(count)?;
            for &index in &carried {
                let value = &self.locals[index].value;
                if !self.alive[value.id as usize] {
                    return Err(self.locate(
                        super::owners::ownership("unavailable scalar loop header argument"),
                        span,
                    ));
                }
                arguments.push(value.id);
            }
            self.blocks[self.block].terminator =
                raw::Terminator::Jump(raw::Edge { target: header_id, arguments });
        }
        // The false path observes header parameters, never body-only identities or loans.
        self.charge_branch_state()?;
        self.locals = saved;
        self.alive = alive;
        self.alive.resize(self.next as usize, false);
        self.loans = loans;
        self.loan_parents = parents;
        self.scope_start = prior_scope;
        self.block = self.new_block(span)?;
        self.blocks[condition_exit].terminator = raw::Terminator::Branch {
            condition: condition.id,
            yes: raw::Edge {
                target: u32::try_from(body_id).map_err(|_| Failure::InternalFailure)?,
                arguments: Vec::new(),
            },
            no: raw::Edge {
                target: u32::try_from(self.block).map_err(|_| Failure::InternalFailure)?,
                arguments: Vec::new(),
            },
        };
        Ok(())
    }
    fn loop_header_arguments(
        &mut self,
        span: UntrustedSpan,
    ) -> Result<(Vec<usize>, Vec<u32>), Failure> {
        let count = self.locals.iter().filter(|b| scalar_place(b)).count();
        if count > crate::data_ownership_v1::MAX_BLOCK_PARAMETERS {
            return Err(crate::generic_v1::budget("scalar loop header parameter ceiling"));
        }
        // Charge indexes, scalar keys, parameter identities and both edge operand vectors.
        self.budget.branch_state(count.checked_mul(5).ok_or(Failure::InternalFailure)?)?;
        let mut carried = reserve(count)?;
        let mut arguments = reserve(count)?;
        for (index, binding) in self.locals.iter().enumerate() {
            if scalar_place(binding) {
                if !self.alive[binding.value.id as usize]
                    || self.loans.values().any(|(root, _)| *root == binding.value.id)
                {
                    return Err(self.locate(
                        super::owners::ownership("loop header cannot replace a borrowed place"),
                        span,
                    ));
                }
                carried.push(index);
                arguments.push(binding.value.id);
            }
        }
        Ok((carried, arguments))
    }
}

fn scalar_place(binding: &super::Binding<'_>) -> bool {
    binding.mutable
        && matches!(&binding.value.ty, Closed::Stored(key) if matches!(key.as_slice(), [0 | 1]))
}
