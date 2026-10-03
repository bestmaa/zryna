//! Exact dense SSA/CFG topology and dominance, independent of producer traversal order.

use super::{Failure, raw, reject, reserve};
use zryna_source::{SourceMap, UntrustedSpan};

pub(super) struct Value {
    pub ty: raw::Type,
    block: usize,
    position: Option<usize>,
}

pub(super) struct Graph {
    values: Vec<Value>,
    enter: Vec<usize>,
    exit: Vec<usize>,
}

impl Graph {
    pub fn operand(&self, id: u32, block: usize, position: usize) -> Result<raw::Type, Failure> {
        let value = self.values.get(id as usize).ok_or_else(|| reject("unknown value operand"))?;
        let available = if value.block == block {
            value.position.is_none_or(|defined| defined < position)
        } else {
            self.enter[value.block] <= self.enter[block]
                && self.exit[block] <= self.exit[value.block]
        };
        if !available {
            return Err(reject("value is forward-referenced or does not dominate its use"));
        }
        Ok(value.ty)
    }
}

pub(super) fn build(
    function: &raw::Function,
    sources: &SourceMap,
    type_count: usize,
) -> Result<Graph, Failure> {
    if function.blocks.is_empty() {
        return Err(reject("closed function has no entry block"));
    }
    let mut values = reserve(0)?;
    let mut successors = reserve(function.blocks.len())?;
    let mut predecessors = reserve(function.blocks.len())?;
    predecessors.resize_with(function.blocks.len(), Vec::new);
    for (block_index, block) in function.blocks.iter().enumerate() {
        if block.id as usize != block_index {
            return Err(reject("block IDs are not dense canonical order"));
        }
        span(sources, function.span, block.span)?;
        for parameter in &block.parameters {
            definition(&mut values, parameter, block_index, None, type_count)?;
        }
        for (position, instruction) in block.instructions.iter().enumerate() {
            span(sources, function.span, instruction.span)?;
            definition(&mut values, &instruction.result, block_index, Some(position), type_count)?;
        }
        let mut next = reserve(2)?;
        for edge in edges(&block.terminator) {
            let target = edge.target as usize;
            if target == 0 || target >= function.blocks.len() || edge.arguments.len() > 256 {
                return Err(reject(
                    "edge targets entry/unknown block or exceeds exact parameter arity",
                ));
            }
            next.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
            next.push(target);
            predecessors[target].try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
            predecessors[target].push(block_index);
        }
        next.sort_unstable();
        next.dedup();
        successors.push(next);
    }
    let entry = &function.blocks[0].parameters;
    if entry.len() != function.parameters.len()
        || entry.iter().zip(&function.parameters).any(|(value, ty)| value.ty != *ty)
    {
        return Err(reject("entry parameters differ from the exact substituted signature"));
    }
    for list in &mut predecessors {
        list.sort_unstable();
        list.dedup();
    }
    let idom = crate::control_flow_v1::immediate_dominators(&successors, &predecessors)
        .ok_or_else(|| reject("CFG has unreachable claims or no complete canonical dominators"))?;
    let (enter, exit) = crate::control_flow_v1::dominator_intervals(&idom);
    super::loops::check(&successors, &predecessors, &enter, &exit)?;
    Ok(Graph { values, enter, exit })
}

fn definition(
    values: &mut Vec<Value>,
    definition: &raw::Definition,
    block: usize,
    position: Option<usize>,
    count: usize,
) -> Result<(), Failure> {
    if definition.id as usize != values.len() {
        return Err(reject("value IDs are not dense in complete block/definition order"));
    }
    ty(definition.ty, count)?;
    values.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
    values.push(Value { ty: definition.ty, block, position });
    Ok(())
}

pub(super) fn ty(ty: raw::Type, count: usize) -> Result<(), Failure> {
    let index = match ty {
        raw::Type::Stored(id) => id,
        raw::Type::Borrow { referent, .. } => referent,
        raw::Type::Unit => return Ok(()),
    };
    if index as usize >= count {
        return Err(reject("value type is absent from the exact closed layout universe"));
    }
    Ok(())
}

pub(super) fn span(
    sources: &SourceMap,
    function: UntrustedSpan,
    at: UntrustedSpan,
) -> Result<(), Failure> {
    if at.file != function.file
        || at.start < function.start
        || at.end > function.end
        || sources.verify_span(at).is_err()
    {
        return Err(reject(
            "body range is foreign, outside its declaration or not a UTF-8 boundary",
        ));
    }
    Ok(())
}

pub(super) fn edges(terminator: &raw::Terminator) -> Vec<&raw::Edge> {
    match terminator {
        raw::Terminator::Return(_) => vec![],
        raw::Terminator::Jump(edge) => vec![edge],
        raw::Terminator::Branch { yes, no, .. } => vec![yes, no],
        raw::Terminator::ClosedEnumMatch { arms, .. } => arms.iter().map(|arm| &arm.edge).collect(),
    }
}
