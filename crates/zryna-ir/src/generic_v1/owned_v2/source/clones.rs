//! Concrete structural clone; a compiler-created shared match retains the named owner.
use super::{Builder, Closed, Failure, Owned, Value, raw, reserve, type_id};
use zryna_source::UntrustedSpan;

const OPTION: &[u8] = &[0x14, 1, 0, 0, 0, 1, 0, 0, 0, 2];
const RESULT: &[u8] = &[0x15, 2, 0, 0, 0, 1, 0, 0, 0, 2, 1, 0, 0, 0, 2];

pub(super) fn admitted(ty: &Closed) -> bool {
    matches!(ty, Closed::Stored(key) if key == OPTION || key == RESULT)
}

impl Builder<'_, '_> {
    pub(super) fn clone_enum(
        &mut self,
        root: Value,
        span: UntrustedSpan,
    ) -> Result<Value, Failure> {
        let Closed::Stored(key) = &root.ty else { return Err(Failure::InternalFailure) };
        // All key copies and both arm/edge/output inventories are charged before allocation.
        self.budget.branch_state(key.len() * 8 + 16)?;
        let ty = if self.symbolic {
            0
        } else {
            let raw::Type::Stored(id) = type_id(self.program, root.ty.clone())? else {
                return Err(Failure::InternalFailure);
            };
            id
        };
        let loan = self.ext(
            Closed::Borrow(key.clone(), false),
            span,
            Owned::Borrow { value: root.id, exclusive: false },
        )?;
        let entry = self.block;
        self.charge_branch_state()?;
        let saved = self.locals.clone();
        let alive = self.alive.clone();
        let loans = self.loans.clone();
        let parents = self.loan_parents.clone();
        let mut arms = reserve(2)?;
        let mut outputs = reserve(2)?;
        for ordinal in 0..2 {
            // No synthetic lexical binding is added, so current binding/key and loan
            // inventories equal the snapshot; next also charges the expanded alive vector.
            self.charge_branch_state()?;
            self.locals = saved.clone();
            self.alive.clone_from(&alive);
            self.alive.resize(self.next as usize, false);
            self.loans = loans.clone();
            self.loan_parents = parents.clone();
            let (arm, block, result) = self.clone_arm(&root, &loan, ty, ordinal, span)?;
            if self.alive[..alive.len()] != alive
                || self.loans != loans
                || self.loan_parents != parents
            {
                return Err(self.locate(
                    super::owners::ownership("structural clone changes incoming owner/loan state"),
                    span,
                ));
            }
            outputs.push((block, result));
            arms.push(arm);
        }
        self.charge_branch_state()?;
        self.locals = saved;
        self.alive = alive;
        self.alive.resize(self.next as usize, false);
        self.loans = loans;
        self.loan_parents = parents;
        self.block = self.new_block(span)?;
        let value = self.value(root.ty)?;
        let definition = self.definition(&value)?;
        self.blocks[self.block]
            .parameters
            .try_reserve(1)
            .map_err(|_| Failure::AllocationFailure)?;
        self.blocks[self.block].parameters.push(definition);
        for (block, id) in outputs {
            let mut arguments = reserve(1)?;
            arguments.push(id);
            self.blocks[block].terminator = raw::Terminator::Jump(raw::Edge {
                target: u32::try_from(self.block).map_err(|_| Failure::InternalFailure)?,
                arguments,
            });
        }
        self.blocks[entry].span = span;
        self.blocks[entry].terminator = raw::Terminator::ClosedEnumMatch {
            scrutinee: loan.id,
            ty,
            mode: raw::MatchMode::SharedBorrow,
            arms,
        };
        self.ext(Closed::Unit, span, Owned::EndLoan(loan.id))?;
        Ok(value)
    }
    fn clone_arm(
        &mut self,
        root: &Value,
        loan: &Value,
        ty: u32,
        ordinal: u32,
        span: UntrustedSpan,
    ) -> Result<(raw::Arm, usize, u32), Failure> {
        self.block = self.new_block(span)?;
        let target = self.block;
        let from = self.next as usize;
        let (binding, payload) = if matches!(&root.ty, Closed::Stored(key) if key == OPTION)
            && ordinal == 0
        {
            (None, None)
        } else {
            let borrowed = self.value(Closed::Borrow(vec![2], false))?;
            let definition = self.definition(&borrowed)?;
            self.blocks[target]
                .parameters
                .try_reserve(1)
                .map_err(|_| Failure::AllocationFailure)?;
            self.blocks[target].parameters.push(definition.clone());
            self.loans.insert(borrowed.id, (root.id, false));
            self.loan_parents.insert(borrowed.id, loan.id);
            let cloned =
                self.ext(Closed::Stored(vec![2]), span, Owned::CloneBorrowedString(borrowed.id))?;
            (Some(definition), Some(cloned.id))
        };
        let value = self.emit(
            root.ty.clone(),
            span,
            raw::Operation::ClosedEnumConstruct { ty, ordinal, payload },
        )?;
        self.close_scope(from, Some(value.id), span)?;
        self.consume(value.id)?;
        let arm = raw::Arm {
            ordinal,
            binding,
            edge: raw::Edge {
                target: u32::try_from(target).map_err(|_| Failure::InternalFailure)?,
                arguments: Vec::new(),
            },
        };
        Ok((arm, self.block, value.id))
    }
}
