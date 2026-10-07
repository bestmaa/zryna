//! Bounded fixture-only observation; no runtime, allocator or emission authority.
use super::*;
/// Test-only bounded semantic observation, not an emitter/runtime/allocator model.
/// Ignores effect placeholders; exact ownership is checked separately against literal plans.
#[derive(Clone, Debug)]
enum Observation {
    Bool(bool),
    I32(i32),
    Enum(u32, Option<Box<Observation>>),
    Unit,
}

pub(super) fn observe(claim: &raw::Program, flag: bool) -> i32 {
    let function = &claim.graph.functions[0];
    let mut values = std::collections::BTreeMap::new();
    values.insert(0, Observation::Unit); // Retained String is never observed here.
    values.insert(1, Observation::Bool(flag));
    let mut bi = 0usize;
    for _ in 0..128 {
        let block = &function.blocks[bi];
        for i in &block.instructions {
            let value = match &i.operation {
                graph::Operation::BoolLiteral(v) => Observation::Bool(*v),
                graph::Operation::I32Literal(v) => Observation::I32(*v),
                graph::Operation::Unit => Observation::Unit,
                graph::Operation::Copy { value } => values[value].clone(),
                graph::Operation::ClosedEnumConstruct { ordinal, payload, .. } => {
                    Observation::Enum(*ordinal, payload.map(|id| Box::new(values[&id].clone())))
                }
                graph::Operation::I32Add { left, right } => {
                    let (Observation::I32(a), Observation::I32(b)) =
                        (&values[left], &values[right])
                    else {
                        panic!("scalar observation")
                    };
                    Observation::I32(a.wrapping_add(*b))
                }
                _ => panic!("hand fixture contains no calls"),
            };
            values.insert(i.result.id, value);
        }
        let (edge, binding) = match &block.terminator {
            graph::Terminator::Return(id) => {
                let Observation::I32(v) = values[id] else { panic!("scalar return") };
                return v;
            }
            graph::Terminator::Jump(edge) => (edge, None),
            graph::Terminator::Branch { condition, yes, no } => {
                let Observation::Bool(v) = values[condition] else { panic!("bool condition") };
                (if v { yes } else { no }, None)
            }
            graph::Terminator::ClosedEnumMatch { scrutinee, arms, .. } => {
                let Observation::Enum(ordinal, payload) = &values[scrutinee] else {
                    panic!("enum observation")
                };
                let arm = &arms[*ordinal as usize];
                assert_eq!(arm.ordinal, *ordinal);
                assert_eq!(arm.binding.is_some(), payload.is_some());
                (&arm.edge, payload.as_deref().cloned())
            }
        };
        let mut incoming = Vec::new();
        if let Some(value) = binding {
            incoming.push(value);
        }
        incoming.extend(edge.arguments.iter().map(|id| values[id].clone()));
        let target = &function.blocks[edge.target as usize];
        assert_eq!(incoming.len(), target.parameters.len());
        for (parameter, value) in target.parameters.iter().zip(incoming) {
            values.insert(parameter.id, value);
        }
        bi = edge.target as usize;
    }
    panic!("finite hand-authored fixture exceeded observation bound")
}
