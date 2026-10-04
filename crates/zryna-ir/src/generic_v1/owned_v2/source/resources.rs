//! Aggregate specialization credit is charged before retaining each generated operation.
use crate::data_ownership_v1 as limits;
use crate::generic_v1::{Failure, budget, raw};

#[derive(Clone, Copy, Default)]
pub(super) struct Budget {
    values: usize,
    blocks: usize,
    calls: usize,
    operands: usize,
    literals: usize,
}
impl Budget {
    pub(super) fn value(&mut self) -> Result<(), Failure> {
        add(&mut self.values, 1, limits::MAX_VALUES_PER_PROGRAM)
    }
    pub(super) fn block(&mut self) -> Result<(), Failure> {
        add(&mut self.blocks, 1, limits::MAX_BLOCKS_PER_PROGRAM)
    }
    pub(super) fn operation(&mut self, operation: &raw::Operation) -> Result<(), Failure> {
        match operation {
            raw::Operation::ClosedGenericCall { .. } | raw::Operation::SourceCall { .. } => {
                add(&mut self.calls, 1, limits::MAX_CALL_EDGES)
            }
            raw::Operation::ClosedEnumConstruct { payload: Some(_), .. } => {
                add(&mut self.operands, 1, limits::MAX_AGGREGATE_OPERANDS)
            }
            _ => Ok(()),
        }
    }
    pub(super) fn literal(&mut self, bytes: usize) -> Result<(), Failure> {
        if bytes > 65_536 {
            return Err(budget("owned String literal ceiling"));
        }
        // Necessary lower bound on encoded size; the wire verifier still checks full overhead.
        add(&mut self.literals, bytes, super::super::wire::MAX_BYTES)
    }
}
fn add(total: &mut usize, extra: usize, ceiling: usize) -> Result<(), Failure> {
    let next = total
        .checked_add(extra)
        .filter(|next| *next <= ceiling)
        .ok_or_else(|| budget("owned source specialization aggregate ceiling"))?;
    *total = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_closed_bodies_stop_before_copying_the_first_excess_literal() {
        let mut credit = Budget::default();
        // 512 legal 64KiB literals exhaust the 32MiB lower bound without allocating them.
        for _ in 0..512 {
            credit.literal(65_536).expect("exact permitted credit");
        }
        assert!(credit.literal(1).is_err());
        assert_eq!(credit.literals, super::super::super::wire::MAX_BYTES);
        assert!(Budget::default().literal(65_537).is_err());
    }

    #[test]
    fn aggregate_graph_credit_survives_function_boundaries_and_rejects_first_extra() {
        let mut credit = Budget::default();
        for _ in 0..16 {
            let mut next_function = credit;
            for _ in 0..16_384 {
                next_function.value().expect("bounded function");
            }
            credit = next_function;
        }
        assert!(credit.value().is_err());
        credit.blocks = limits::MAX_BLOCKS_PER_PROGRAM;
        credit.calls = limits::MAX_CALL_EDGES;
        credit.operands = limits::MAX_AGGREGATE_OPERANDS;
        assert!(credit.block().is_err());
        assert!(
            credit
                .operation(&raw::Operation::SourceCall {
                    module: 0,
                    function: 0,
                    arguments: Vec::new(),
                })
                .is_err()
        );
        assert!(
            credit
                .operation(&raw::Operation::ClosedEnumConstruct {
                    ty: 0,
                    ordinal: 0,
                    payload: Some(0),
                })
                .is_err()
        );
        let mut overflowing = usize::MAX;
        assert!(add(&mut overflowing, 1, usize::MAX).is_err());
    }
}
