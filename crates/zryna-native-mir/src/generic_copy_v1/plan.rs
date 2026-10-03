//! Complete private shape preflight and iterative reachable reverse postorder.

use super::{
    FunctionPlan, MAX_CODEGEN_UNITS, MAX_PARAMETERS, MAX_VALUE_LANES, budget, invariant, width,
};
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::raw;
use zryna_layout::generic_v1::TypeView;

pub(super) fn function(
    index: usize,
    f: &raw::Function,
    types: &[TypeView<'_>],
    cache: &mut [Option<u32>],
    units: &mut u64,
) -> Result<FunctionPlan, Diagnostic> {
    let result = width(f.result, types, cache, 0)?;
    let mut parameters = 0u32;
    for ty in &f.parameters {
        parameters = parameters.checked_add(width(*ty, types, cache, 0)?).ok_or_else(budget)?;
    }
    if parameters.checked_add(1).is_none_or(|n| n > MAX_PARAMETERS) {
        return Err(budget());
    }
    let mut lanes = 0u32;
    for block in &f.blocks {
        for def in block.parameters.iter().chain(block.instructions.iter().map(|i| &i.result)) {
            lanes = lanes.checked_add(width(def.ty, types, cache, 0)?).ok_or_else(budget)?;
        }
        take(units, 16)?;
        for i in &block.instructions {
            let n = match &i.operation {
                raw::Operation::ClosedGenericCall { arguments, .. }
                | raw::Operation::SourceCall { arguments, .. } => arguments.len(),
                _ => 1,
            };
            take(
                units,
                u64::try_from(n.max(1))
                    .map_err(|_| budget())?
                    .checked_mul(256)
                    .ok_or_else(budget)?,
            )?;
        }
        for edge in edges(&block.terminator) {
            take(
                units,
                u64::try_from(edge.arguments.len())
                    .map_err(|_| budget())?
                    .checked_mul(256)
                    .ok_or_else(budget)?,
            )?;
        }
    }
    if lanes > MAX_VALUE_LANES {
        return Err(budget());
    }
    take(units, u64::from(lanes).checked_mul(4).ok_or_else(budget)?)?;
    Ok(FunctionPlan {
        symbol: format!("zryna_gcopy_v1_{index}"),
        parameters,
        result,
        order: order(f)?,
    })
}

fn take(units: &mut u64, n: u64) -> Result<(), Diagnostic> {
    *units = units.checked_add(n).filter(|n| *n <= MAX_CODEGEN_UNITS).ok_or_else(budget)?;
    Ok(())
}

fn edges(t: &raw::Terminator) -> Vec<&raw::Edge> {
    match t {
        raw::Terminator::Return(_) => Vec::new(),
        raw::Terminator::Jump(e) => vec![e],
        raw::Terminator::Branch { yes, no, .. } => vec![yes, no],
        raw::Terminator::ClosedEnumMatch { arms, .. } => arms.iter().map(|a| &a.edge).collect(),
    }
}

fn order(f: &raw::Function) -> Result<Vec<usize>, Diagnostic> {
    let mut visited = Vec::new();
    visited.try_reserve(f.blocks.len()).map_err(|_| budget())?;
    visited.resize(f.blocks.len(), false);
    let mut pending = Vec::new();
    pending.try_reserve(f.blocks.len().checked_mul(2).ok_or_else(budget)?).map_err(|_| budget())?;
    let mut result = Vec::new();
    result.try_reserve(f.blocks.len()).map_err(|_| budget())?;
    pending.push((0, false));
    while let Some((index, exit)) = pending.pop() {
        if exit {
            result.push(index);
            continue;
        }
        let mark = visited.get_mut(index).ok_or_else(invariant)?;
        if *mark {
            continue;
        }
        *mark = true;
        pending.push((index, true));
        for edge in edges(&f.blocks[index].terminator).into_iter().rev() {
            pending.push((edge.target as usize, false));
        }
    }
    result.reverse();
    Ok(result)
}

#[cfg(test)]
mod tests {
    #[test]
    fn exact_complete_work_budget_and_first_extra_are_checked() {
        let mut n = 0;
        super::take(&mut n, super::MAX_CODEGEN_UNITS).expect("exact ceiling");
        assert_eq!(super::take(&mut n, 1).expect_err("extra").code, "ZRYNA-N7001");
        let mut overflow = u64::MAX;
        assert_eq!(super::take(&mut overflow, 1).expect_err("overflow").code, "ZRYNA-N7001");
    }
}
