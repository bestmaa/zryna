//! Reducibility and inherited natural-loop nesting of the complete closed CFG.

use super::{Failure, budget, reject, reserve};

pub(super) fn check(
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    enter: &[usize],
    exit: &[usize],
) -> Result<(), Failure> {
    let count = successors.len();
    let dominates = |a: usize, b: usize| enter[a] <= enter[b] && exit[b] <= exit[a];
    let mut forward = reserve(count)?;
    forward.resize_with(count, Vec::new);
    let mut degree = reserve(count)?;
    degree.resize(count, 0usize);
    let mut loops = reserve(count)?;
    loops.resize_with(count, Vec::new);
    for (source, targets) in successors.iter().enumerate() {
        for &target in targets {
            if dominates(target, source) {
                if loops[target].is_empty() {
                    loops[target] = reserve(count)?;
                    loops[target].resize(count, false);
                }
                loops[target][target] = true;
                let mut pending = reserve(count)?;
                if !loops[target][source] {
                    loops[target][source] = true;
                    pending.push(source);
                }
                while let Some(node) = pending.pop() {
                    for &previous in &predecessors[node] {
                        if !loops[target][previous] {
                            loops[target][previous] = true;
                            pending.push(previous);
                        }
                    }
                }
            } else {
                forward[source].try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
                forward[source].push(target);
                degree[target] = degree[target].checked_add(1).ok_or(Failure::InternalFailure)?;
            }
        }
    }
    let mut pending = reserve(count)?;
    for (id, value) in degree.iter().enumerate() {
        if *value == 0 {
            pending.push(id);
        }
    }
    let mut next = 0usize;
    while next < pending.len() {
        let source = pending[next];
        next += 1;
        for &target in &forward[source] {
            degree[target] = degree[target].checked_sub(1).ok_or(Failure::InternalFailure)?;
            if degree[target] == 0 {
                pending.push(target);
            }
        }
    }
    if pending.len() != count {
        return Err(reject("closed CFG is irreducible after removing dominating backedges"));
    }
    let mut depths = reserve(count)?;
    depths.resize(count, 0usize);
    for members in loops.iter().filter(|members| !members.is_empty()) {
        for (node, present) in members.iter().copied().enumerate() {
            if present {
                depths[node] = depths[node].checked_add(1).ok_or(Failure::InternalFailure)?;
                if depths[node] > crate::data_ownership_v1::MAX_LOOP_NESTING {
                    return Err(budget("inherited loop nesting limit 128; first extra 129"));
                }
            }
        }
    }
    Ok(())
}
