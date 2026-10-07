//! Source-bound deviations. No assertion that same-type swaps fail independent typing.
use super::{Failure, owned_v2, raw};

fn rejects_with_recovery(
    pristine: &owned_v2::raw::Program,
    hostile: &owned_v2::raw::Program,
    check: &impl Fn(&owned_v2::raw::Program) -> Result<usize, Failure>,
) {
    assert!(matches!(check(hostile), Err(Failure::Diagnostics(_))));
    assert_eq!(check(pristine).expect("fresh pristine source/plan recovery"), 9);
}

pub(super) fn source_bound_attacks(
    pristine: &owned_v2::raw::Program,
    fi: usize,
    bi: usize,
    check: &impl Fn(&owned_v2::raw::Program) -> Result<usize, Failure>,
) {
    pair_swaps(pristine, fi, bi, check);
    literal_changes(pristine, check);
    structural_changes(pristine, fi, bi, check);
    cleanup_changes(pristine, check);
}

fn pair_swaps(
    pristine: &owned_v2::raw::Program,
    fi: usize,
    bi: usize,
    check: &impl Fn(&owned_v2::raw::Program) -> Result<usize, Failure>,
) {
    let target = pristine.graph.functions[fi].blocks[bi].id;
    let incoming: Vec<_> = pristine.graph.functions[fi]
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| match &block.terminator {
            raw::Terminator::Jump(edge) if edge.target == target => Some(index),
            _ => None,
        })
        .collect();
    assert_eq!(incoming.len(), 2);
    for index in incoming {
        for mask in [1, 2, 3] {
            let mut hostile = pristine.clone();
            let raw::Terminator::Jump(edge) =
                &mut hostile.graph.functions[fi].blocks[index].terminator
            else {
                panic!("incoming edge")
            };
            assert_ne!(edge.arguments[0], edge.arguments[1]);
            assert_ne!(edge.arguments[2], edge.arguments[3]);
            if mask & 1 != 0 {
                edge.arguments.swap(0, 1);
            }
            if mask & 2 != 0 {
                edge.arguments.swap(2, 3);
            }
            // Same exact parameter types and existing dominating operands are retained.
            // Rejection must authenticate source transport; typing is a separate proof.
            assert_eq!(hostile.plans, pristine.plans);
            rejects_with_recovery(pristine, &hostile, check);
        }
    }
}

fn literal_changes(
    pristine: &owned_v2::raw::Program,
    check: &impl Fn(&owned_v2::raw::Program) -> Result<usize, Failure>,
) {
    for boolean in [false, true] {
        let mut hostile = pristine.clone();
        let instruction = hostile
            .graph
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| {
                if boolean {
                    matches!(instruction.operation, raw::Operation::BoolLiteral(true))
                } else {
                    matches!(instruction.operation, raw::Operation::I32Literal(11))
                }
            })
            .expect("fixed source literal");
        instruction.operation = if boolean {
            raw::Operation::BoolLiteral(false)
        } else {
            raw::Operation::I32Literal(12)
        };
        assert_eq!(hostile.plans, pristine.plans);
        rejects_with_recovery(pristine, &hostile, check);
    }
}

fn structural_changes(
    pristine: &owned_v2::raw::Program,
    fi: usize,
    bi: usize,
    check: &impl Fn(&owned_v2::raw::Program) -> Result<usize, Failure>,
) {
    for mutation in 0..6 {
        let mut hostile = pristine.clone();
        let target = hostile.graph.functions[fi].blocks[bi].id;
        if mutation < 3 {
            let edge = hostile.graph.functions[fi]
                .blocks
                .iter_mut()
                .find_map(|block| match &mut block.terminator {
                    raw::Terminator::Jump(edge) if edge.target == target => Some(edge),
                    _ => None,
                })
                .expect("incoming");
            match mutation {
                0 => {
                    edge.arguments.pop();
                }
                1 => edge.arguments.push(edge.arguments[0]),
                _ => edge.target = 0,
            }
        } else if mutation == 3 {
            let block = hostile.graph.functions[fi]
                .blocks
                .iter_mut()
                .find(|b| matches!(b.terminator, raw::Terminator::Return(_)))
                .expect("actual return");
            block.terminator = raw::Terminator::Return(u32::MAX);
        } else if mutation == 4 {
            let raw::Type::Stored(id) = hostile.graph.functions[fi].blocks[bi].parameters[0].ty
            else {
                panic!("stored enum")
            };
            hostile.graph.type_keys[id as usize].push(0);
        } else {
            let constructor = hostile
                .graph
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.blocks)
                .flat_map(|block| &mut block.instructions)
                .find(|instruction| {
                    matches!(
                        instruction.operation,
                        raw::Operation::ClosedEnumConstruct { payload: None, .. }
                    )
                })
                .expect("actual None constructor");
            let raw::Operation::ClosedEnumConstruct { ordinal, .. } = &mut constructor.operation
            else {
                panic!("constructor")
            };
            *ordinal = 1; // Some requires a payload; missing payload cannot gain authority.
        }
        rejects_with_recovery(pristine, &hostile, check);
    }
}

fn cleanup_changes(
    pristine: &owned_v2::raw::Program,
    check: &impl Fn(&owned_v2::raw::Program) -> Result<usize, Failure>,
) {
    for mutation in 0..4 {
        let mut hostile = pristine.clone();
        let step = hostile
            .plans
            .iter_mut()
            .flat_map(|plan| &mut plan.steps)
            .find(|step| step.failure && step.cleanup.len() >= 2)
            .expect("retained roots");
        match mutation {
            0 => {
                step.cleanup.pop();
            }
            1 => step.cleanup.push(step.cleanup[0]),
            2 => step.cleanup.swap(0, 1),
            _ => step.cleanup.push(u32::MAX),
        }
        rejects_with_recovery(pristine, &hostile, check);
    }
}
