//! Independent used-original call cycle and inherited static-depth checks.

use super::{Failure, budget, reject, reserve};

pub(super) fn check(count: usize, calls: &[(usize, usize)]) -> Result<(), Failure> {
    let mut successors = reserve(count)?;
    successors.resize_with(count, Vec::new);
    let mut degrees = reserve(count)?;
    degrees.resize(count, 0usize);
    for &(source, target) in calls {
        successors[source].try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
        successors[source].push(target);
    }
    for targets in &mut successors {
        targets.sort_unstable();
        targets.dedup();
        for &target in targets.iter() {
            degrees[target] = degrees[target].checked_add(1).ok_or(Failure::InternalFailure)?;
        }
    }
    let mut pending = reserve(count)?;
    for (id, degree) in degrees.iter().enumerate() {
        if *degree == 0 {
            pending.push(id);
        }
    }
    let mut depth = reserve(count)?;
    depth.resize(count, 1usize);
    let (mut next, mut seen) = (0usize, 0usize);
    while next < pending.len() {
        let source = pending[next];
        next += 1;
        seen += 1;
        for &target in &successors[source] {
            depth[target] =
                depth[target].max(depth[source].checked_add(1).ok_or(Failure::InternalFailure)?);
            if depth[target] > crate::data_ownership_v1::MAX_STATIC_CALL_DEPTH {
                return Err(budget("inherited static call depth limit 128; first extra 129"));
            }
            degrees[target] = degrees[target].checked_sub(1).ok_or(Failure::InternalFailure)?;
            if degrees[target] == 0 {
                pending.push(target);
            }
        }
    }
    if seen != count {
        return Err(reject(
            "used original function call graph is recursive, regardless of argument keys",
        ));
    }
    Ok(())
}
