//! Concrete structural clone; a compiler-created shared match retains the named owner.
use super::{Builder, Closed, Failure, Owned, Value, raw, reserve, type_id};
use zryna_source::UntrustedSpan;

const OPTION: &[u8] = &[0x14, 1, 0, 0, 0, 1, 0, 0, 0, 2];
const RESULT: &[u8] = &[0x15, 2, 0, 0, 0, 1, 0, 0, 0, 2, 1, 0, 0, 0, 2];
const OPTION_OPTION: &[u8] = &[0x14, 1, 0, 0, 0, 10, 0, 0, 0, 0x14, 1, 0, 0, 0, 1, 0, 0, 0, 2];
const RESULT_OPTION: &[u8] = &[
    0x15, 2, 0, 0, 0, 10, 0, 0, 0, 0x14, 1, 0, 0, 0, 1, 0, 0, 0, 2, 10, 0, 0, 0, 0x14, 1, 0, 0, 0,
    1, 0, 0, 0, 2,
];
const RESULT_OPTION_STRING: &[u8] =
    &[0x15, 2, 0, 0, 0, 10, 0, 0, 0, 0x14, 1, 0, 0, 0, 1, 0, 0, 0, 2, 1, 0, 0, 0, 2];
const RESULT_STRING_OPTION: &[u8] =
    &[0x15, 2, 0, 0, 0, 1, 0, 0, 0, 2, 10, 0, 0, 0, 0x14, 1, 0, 0, 0, 1, 0, 0, 0, 2];

pub(super) fn admitted(ty: &Closed) -> bool {
    matches!(ty, Closed::Stored(key) if key == OPTION || key == RESULT
        || key == OPTION_OPTION || key == RESULT_OPTION
        || key == RESULT_OPTION_STRING || key == RESULT_STRING_OPTION)
}

fn payload_key(stored: &Closed, ordinal: u32) -> Result<&'static [u8], Failure> {
    if ordinal > 1 {
        return Err(Failure::InternalFailure);
    }
    match stored {
        Closed::Stored(key) if key == OPTION || key == RESULT => Ok(&[2]),
        Closed::Stored(key) if key == OPTION_OPTION || key == RESULT_OPTION => Ok(OPTION),
        Closed::Stored(key) if key == RESULT_OPTION_STRING => {
            Ok(if ordinal == 0 { OPTION } else { &[2] })
        }
        Closed::Stored(key) if key == RESULT_STRING_OPTION => {
            Ok(if ordinal == 0 { &[2] } else { OPTION })
        }
        _ => Err(Failure::InternalFailure),
    }
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
        let loan = self.ext(
            Closed::Borrow(key.clone(), false),
            span,
            Owned::Borrow { value: root.id, exclusive: false },
        )?;
        let value = self.clone_selection(root.id, root.ty, &loan, span)?;
        self.ext(Closed::Unit, span, Owned::EndLoan(loan.id))?;
        Ok(value)
    }

    // Only an admitted outer form can enter; its sole enum child is exact Option<String>.
    // The internal loan is matched, never exposed as a source clone(Borrow) capability.
    fn clone_selection(
        &mut self,
        owner: u32,
        stored: Closed,
        loan: &Value,
        span: UntrustedSpan,
    ) -> Result<Value, Failure> {
        let ty = if self.symbolic {
            0
        } else {
            let raw::Type::Stored(id) = type_id(self.program, stored.clone())? else {
                return Err(Failure::InternalFailure);
            };
            id
        };
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
            let (arm, block, result) = self.clone_arm(owner, &stored, loan, ty, ordinal, span)?;
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
        let value = self.value(stored)?;
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
        Ok(value)
    }
    fn clone_arm(
        &mut self,
        owner: u32,
        stored: &Closed,
        loan: &Value,
        ty: u32,
        ordinal: u32,
        span: UntrustedSpan,
    ) -> Result<(raw::Arm, usize, u32), Failure> {
        self.block = self.new_block(span)?;
        let target = self.block;
        let from = self.next as usize;
        let (binding, payload) = if matches!(stored, Closed::Stored(key) if key == OPTION || key == OPTION_OPTION)
            && ordinal == 0
        {
            (None, None)
        } else {
            let key = payload_key(stored, ordinal)?;
            if key == OPTION {
                // Both payload key copies precede nested selection's own complete key credit.
                self.budget.branch_state(key.len() * 2)?;
            }
            let borrowed = self.value(Closed::Borrow(key.to_vec(), false))?;
            let definition = self.definition(&borrowed)?;
            self.blocks[target]
                .parameters
                .try_reserve(1)
                .map_err(|_| Failure::AllocationFailure)?;
            self.blocks[target].parameters.push(definition.clone());
            self.loans.insert(borrowed.id, (owner, false));
            self.loan_parents.insert(borrowed.id, loan.id);
            let cloned = if key == OPTION {
                self.budget.branch_state(key.len() * 8 + 16)?;
                self.clone_selection(owner, Closed::Stored(key.to_vec()), &borrowed, span)?
            } else {
                self.ext(Closed::Stored(vec![2]), span, Owned::CloneBorrowedString(borrowed.id))?
            };
            (Some(definition), Some(cloned.id))
        };
        let value = self.emit(
            stored.clone(),
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
