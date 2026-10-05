//! Independent path replay, owner affinity, payload loan refinement and complete cleanup.
use super::{Failure, graph, raw};
use crate::generic_v1::{body, budget, cfg, reject, reserve};
use zryna_layout::generic_v1::VerifiedLayouts;
mod state;
mod topology;
use state::State;
/// Complete owner state copies and cleanup action credits per invocation of this checker.
pub const MAX_OWNER_UNITS: usize = 1_048_576;
fn charge(used: &mut usize, n: usize) -> Result<(), Failure> {
    *used = used
        .checked_add(n)
        .filter(|v| *v <= MAX_OWNER_UNITS)
        .ok_or_else(|| budget("owned plan/state amplification ceiling"))?;
    Ok(())
}
fn owned(ty: graph::Type, layouts: &[bool]) -> bool {
    match ty {
        graph::Type::Stored(i) => layouts[i as usize],
        _ => false,
    }
}
fn step(
    plan: &mut raw::Plan,
    state: &State,
    bi: usize,
    pi: usize,
    failure: bool,
    used: &mut usize,
) -> Result<(), Failure> {
    charge(used, state.owners.len() + state.loans.len() + 1)?;
    plan.steps.push(raw::Step {
        block: u32::try_from(bi).map_err(|_| Failure::InternalFailure)?,
        position: u32::try_from(pi).map_err(|_| Failure::InternalFailure)?,
        end_loans: state.loans.keys().rev().copied().collect(),
        cleanup: state.order.iter().rev().copied().collect(),
        failure,
    });
    Ok(())
}
#[allow(
    clippy::too_many_lines,
    reason = "One explicit path replay keeps failure cleanup and owner/loan merges together."
)]
pub(super) fn derive(
    program: &graph::Program,
    extensions: &[Vec<raw::Extension>],
    layouts: &VerifiedLayouts,
) -> Result<Vec<raw::Plan>, Failure> {
    if extensions.len() != program.functions.len() {
        return Err(reject("missing owned effects"));
    }
    let ownership = layouts.types().map(|v| v.drop_kind() != 0).collect::<Vec<_>>();
    let generic_count = program.functions.iter().take_while(|f| f.key[0] == 0x40).count();
    let mut plans = reserve(program.functions.len())?;
    let mut used = 0;
    for (fi, f) in program.functions.iter().enumerate() {
        let types = f
            .blocks
            .iter()
            .flat_map(|b| {
                b.parameters.iter().map(|p| p.ty).chain(b.instructions.iter().map(|i| i.result.ty))
            })
            .collect::<Vec<_>>();
        let mut topology = topology::build(f, &mut used)?;
        let mut states = vec![None; f.blocks.len()];
        let mut headers = vec![None; f.blocks.len()];
        let mut entry = State::default();
        for p in &f.blocks[0].parameters {
            entry.add(p.id, owned(p.ty, &ownership));
            if let graph::Type::Borrow { exclusive, .. } = p.ty {
                entry.loan(p.id, p.id, exclusive, None)?;
            }
        }
        states[0] = Some(entry);
        let mut ready = vec![0usize];
        let mut next = 0;
        let mut plan = raw::Plan::default();
        while next < ready.len() {
            let bi = ready[next];
            next += 1;
            let b = &f.blocks[bi];
            let mut state = states[bi].take().ok_or(Failure::InternalFailure)?;
            if topology.headers[bi] {
                charge(&mut used, state.owners.len() + state.loans.len() + state.order.len() + 1)?;
                headers[bi] = Some(state.clone());
            }
            for (pi, i) in b.instructions.iter().enumerate() {
                let ty = |id: u32| {
                    types.get(id as usize).copied().ok_or_else(|| reject("unknown owner operand"))
                };
                let ext = extensions[fi]
                    .binary_search_by_key(&i.result.id, |e| e.result)
                    .ok()
                    .map(|j| &extensions[fi][j].operation);
                if let Some(op) = ext {
                    match op {
                        raw::Operation::StringLiteral(_) => {
                            step(&mut plan, &state, bi, pi, true, &mut used)?;
                        }
                        raw::Operation::Move(id) => {
                            state.read(*id, ty(*id)?, owned(ty(*id)?, &ownership))?;
                            state.consume(*id, owned(ty(*id)?, &ownership))?;
                        }
                        raw::Operation::Borrow { value, exclusive } => {
                            state.read(*value, ty(*value)?, owned(ty(*value)?, &ownership))?;
                            state.loan(i.result.id, *value, *exclusive, None)?;
                        }
                        raw::Operation::EndLoan(id) => state.end(*id)?,
                        raw::Operation::Drop(id) => state.consume(*id, true)?,
                        raw::Operation::CloneString(id) => {
                            state.read(*id, ty(*id)?, true)?;
                            step(&mut plan, &state, bi, pi, true, &mut used)?;
                        }
                        raw::Operation::CloneBorrowedString(id) => {
                            if ty(*id)? != (graph::Type::Borrow { referent: 2, exclusive: false })
                                || state.loans.get(id).is_none_or(|loan| loan.exclusive)
                            {
                                return Err(reject(
                                    "selected clone requires a live shared String loan",
                                ));
                            }
                            state.read(*id, ty(*id)?, false)?;
                            // Capture failure before the fresh result becomes an owner.
                            step(&mut plan, &state, bi, pi, true, &mut used)?;
                        }
                    }
                } else {
                    match &i.operation {
                        graph::Operation::Copy { value } => {
                            state.read(*value, ty(*value)?, owned(ty(*value)?, &ownership))?;
                        }
                        graph::Operation::I32Add { left, right } => {
                            state.read(*left, ty(*left)?, false)?;
                            state.read(*right, ty(*right)?, false)?;
                        }
                        graph::Operation::ClosedEnumConstruct { payload: Some(id), .. } => {
                            state.read(*id, ty(*id)?, owned(ty(*id)?, &ownership))?;
                            state.consume(*id, owned(ty(*id)?, &ownership))?;
                        }
                        graph::Operation::ClosedGenericCall { arguments, .. }
                        | graph::Operation::SourceCall { arguments, .. } => {
                            state.aliases(arguments)?;
                            for id in arguments {
                                state.read(*id, ty(*id)?, owned(ty(*id)?, &ownership))?;
                                state.consume(*id, owned(ty(*id)?, &ownership))?;
                            }
                            body::call_target(program, generic_count, &i.operation)?
                                .ok_or(Failure::InternalFailure)?;
                            step(&mut plan, &state, bi, pi, true, &mut used)?;
                        }
                        _ => {}
                    }
                }
                state.add(i.result.id, owned(i.result.ty, &ownership));
            }
            if let graph::Terminator::Return(id) = b.terminator {
                let ty = types[id as usize];
                state.read(id, ty, owned(ty, &ownership))?;
                state.consume(id, owned(ty, &ownership))?;
                step(&mut plan, &state, bi, b.instructions.len(), false, &mut used)?;
                continue;
            }
            let mut successors = Vec::new();
            match &b.terminator {
                graph::Terminator::Jump(e) => successors.push((e, None, None)),
                graph::Terminator::Branch { condition, yes, no } => {
                    state.read(*condition, types[*condition as usize], false)?;
                    successors.push((yes, None, None));
                    successors.push((no, None, None));
                }
                graph::Terminator::ClosedEnumMatch { scrutinee, mode, arms, .. } => {
                    state.read(
                        *scrutinee,
                        types[*scrutinee as usize],
                        owned(types[*scrutinee as usize], &ownership),
                    )?;
                    if *mode == graph::MatchMode::Value {
                        state.consume(*scrutinee, owned(types[*scrutinee as usize], &ownership))?;
                    }
                    for arm in arms {
                        successors.push((
                            &arm.edge,
                            arm.binding.as_ref(),
                            if *mode == graph::MatchMode::Value { None } else { Some(*scrutinee) },
                        ));
                    }
                }
                graph::Terminator::Return(_) => return Err(Failure::InternalFailure),
            }
            for (edge, binding, parent) in successors {
                charge(&mut used, state.owners.len() + state.loans.len() + state.order.len() + 1)?;
                let mut incoming = state.clone();
                let target = &f.blocks[edge.target as usize];
                let offset = usize::from(binding.is_some());
                if let Some(binding) = binding {
                    incoming.add(binding.id, owned(binding.ty, &ownership));
                    if let Some(parent) = parent {
                        let l = incoming
                            .loans
                            .get(&parent)
                            .ok_or_else(|| reject("borrowed match lacks retained loan"))?
                            .clone();
                        incoming.loan(binding.id, l.root, l.exclusive, Some(parent))?;
                    }
                }
                for (id, param) in edge.arguments.iter().zip(target.parameters.iter().skip(offset))
                {
                    let ty = types[*id as usize];
                    if matches!(ty, graph::Type::Borrow { .. }) {
                        return Err(reject("loan-carrying ordinary CFG edge"));
                    }
                    incoming.read(*id, ty, owned(ty, &ownership))?;
                    incoming.consume(*id, owned(ty, &ownership))?;
                    incoming.add(param.id, owned(param.ty, &ownership));
                }
                let ti = edge.target as usize;
                if topology.backedge(bi, ti) {
                    let prior = headers[ti].as_ref().ok_or_else(|| {
                        reject("owned backedge lacks its independently replayed header")
                    })?;
                    if prior != &incoming {
                        return Err(reject("owner/loan state differs at loop backedge"));
                    }
                    continue;
                }
                if let Some(prior) = &states[ti] {
                    if prior != &incoming {
                        return Err(reject("owner/loan state differs at CFG merge"));
                    }
                } else {
                    states[ti] = Some(incoming);
                }
                topology.degree[ti] -= 1;
                if topology.degree[ti] == 0 {
                    ready.push(ti);
                }
            }
        }
        if next != f.blocks.len() {
            return Err(reject("owned graph is irreducible or has unavailable forward state"));
        }
        plan.steps.sort_by_key(|s| (s.block, s.position, s.failure));
        plans.push(plan);
    }
    Ok(plans)
}
#[cfg(test)]
#[path = "plan/clones.rs"]
mod clone_tests;
#[cfg(test)]
#[path = "plan/loops.rs"]
mod loop_tests;
#[cfg(test)]
#[path = "plan/tests.rs"]
mod tests;
#[cfg(test)]
mod credit_tests {
    #[test]
    fn complete_owner_state_and_cleanup_credits_reject_first_extra_and_overflow() {
        let mut credits = 0;
        super::charge(&mut credits, super::MAX_OWNER_UNITS).expect("exact complete credit ceiling");
        assert!(
            matches!(super::charge(&mut credits,1),Err(super::Failure::Diagnostics(v)) if v[0].code=="ZRYNA-I3201")
        );
        let mut overflow = usize::MAX;
        assert!(super::charge(&mut overflow, 1).is_err());
    }
}
