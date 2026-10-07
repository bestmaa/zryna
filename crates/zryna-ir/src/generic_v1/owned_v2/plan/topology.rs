//! One bounded forward replay; only independently dominating backedges bypass scheduling.
use super::{Failure, cfg, charge, graph, reject};
use crate::generic_v1::{budget, reserve};

pub(super) struct Topology {
    pub degree: Vec<usize>,
    pub headers: Vec<bool>,
    enter: Vec<usize>,
    exit: Vec<usize>,
}

impl Topology {
    pub fn backedge(&self, source: usize, target: usize) -> bool {
        self.enter[target] <= self.enter[source] && self.exit[source] <= self.exit[target]
    }
}

pub(super) fn build(f: &graph::Function, used: &mut usize) -> Result<Topology, Failure> {
    let count = f.blocks.len();
    if count == 0 {
        return Err(reject("owned function lacks an entry block"));
    }
    // Charge topology inventories before allocating them. The typed CFG still owns dense
    // definitions, exact edge signatures, loop nesting and every operand's dominance.
    charge(used, count.checked_mul(8).ok_or_else(|| budget("owned topology credit overflow"))?)?;
    let mut successors = reserve(count)?;
    successors.resize_with(count, Vec::new);
    let mut predecessors = reserve(count)?;
    predecessors.resize_with(count, Vec::new);
    for (source, block) in f.blocks.iter().enumerate() {
        for edge in cfg::edges(&block.terminator) {
            let target = edge.target as usize;
            if target == 0 || target >= count {
                return Err(reject("owned edge targets entry or an unknown block"));
            }
            charge(used, 2)?;
            successors[source].try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
            predecessors[target].try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
            successors[source].push(target);
            predecessors[target].push(source);
        }
    }
    for list in successors.iter_mut().chain(&mut predecessors) {
        list.sort_unstable();
        list.dedup();
    }
    let idom = crate::control_flow_v1::immediate_dominators(&successors, &predecessors)
        .ok_or_else(|| reject("owned CFG has unreachable claims or no complete dominators"))?;
    let (enter, exit) = crate::control_flow_v1::dominator_intervals(&idom);
    let mut topology = Topology { degree: reserve(count)?, headers: reserve(count)?, enter, exit };
    topology.degree.resize(count, 0usize);
    topology.headers.resize(count, false);
    // Count raw edges, including repeated successors, exactly as the replay visits them.
    for (source, block) in f.blocks.iter().enumerate() {
        for edge in cfg::edges(&block.terminator) {
            let target = edge.target as usize;
            if topology.backedge(source, target) {
                topology.headers[target] = true;
            } else {
                topology.degree[target] += 1;
            }
        }
    }
    Ok(topology)
}
