//! Canonical simple witnesses within the retained declaration-generated graph.

use super::model::Builder;
use super::{InstantiationFailure, TypeShape, push, reserve};

pub(super) fn path(
    builder: &Builder<'_, '_, '_>,
    from: usize,
    to: usize,
) -> Result<Option<Vec<usize>>, InstantiationFailure> {
    let TypeShape::Nominal(target) = builder.types[to].shape else {
        return Ok(None);
    };
    if !builder.generic(builder.types[to].shape) {
        return Ok(None);
    }
    let mut keys = reserve(builder.types.len())?;
    keys.extend(builder.types.iter().map(|node| node.key.as_slice()));
    let mut origins = reserve(builder.types.len())?;
    origins.extend(builder.types.iter().enumerate().filter_map(|(id, node)| {
        (id != to && node.shape == TypeShape::Nominal(target)).then_some(id)
    }));
    canonical_path(&keys, &builder.generated_reverse, &origins, from, to)
}

fn canonical_path(
    keys: &[&[u8]],
    reverse: &[Vec<usize>],
    origins: &[usize],
    from: usize,
    to: usize,
) -> Result<Option<Vec<usize>>, InstantiationFailure> {
    let mut reachable = reserve(keys.len())?;
    reachable.resize(keys.len(), false);
    let mut pending = reserve(keys.len())?;
    pending.push(from);
    while let Some(id) = pending.pop() {
        if reachable[id] {
            continue;
        }
        reachable[id] = true;
        for &parent in &reverse[id] {
            push(&mut pending, parent)?;
        }
    }
    let Some(root) =
        origins.iter().copied().filter(|id| reachable[*id]).min_by(|a, b| keys[*a].cmp(keys[*b]))
    else {
        return Ok(None);
    };
    let mut forward = reserve(keys.len())?;
    forward.resize_with(keys.len(), Vec::new);
    for (child, parents) in reverse.iter().enumerate().filter(|(id, _)| reachable[*id]) {
        for &parent in parents {
            if reachable[parent] {
                push(&mut forward[parent], child)?;
            }
        }
    }
    for children in &mut forward {
        children.sort_unstable_by(|a, b| keys[*a].cmp(keys[*b]));
        children.dedup();
    }
    reachable.fill(false);
    let mut stack = reserve(keys.len())?;
    reachable[root] = true;
    stack.push((root, 0));
    while let Some((id, next)) = stack.last_mut() {
        if *id == from {
            let mut path =
                reserve(stack.len().checked_add(1).ok_or(InstantiationFailure::InternalFailure)?)?;
            path.extend(stack.into_iter().map(|(id, _)| id));
            path.push(to);
            return Ok(Some(path));
        }
        if *next == forward[*id].len() {
            stack.pop();
            continue;
        }
        let child = forward[*id][*next];
        *next += 1;
        if !reachable[child] {
            reachable[child] = true;
            push(&mut stack, (child, 0))?;
        }
    }
    Err(InstantiationFailure::InternalFailure)
}

#[cfg(test)]
mod tests {
    use super::canonical_path;

    fn enumerate(
        forward: &[Vec<usize>],
        path: &mut Vec<usize>,
        destination: usize,
        found: &mut Vec<Vec<usize>>,
    ) {
        let current = *path.last().expect("nonempty oracle path");
        if current == destination {
            found.push(path.clone());
            return;
        }
        for &child in &forward[current] {
            if !path.contains(&child) {
                path.push(child);
                enumerate(forward, path, destination, found);
                path.pop();
            }
        }
    }

    #[test]
    fn synthetic_small_graphs_match_an_independent_complete_simple_path_oracle() {
        let keys: &[&[u8]] = &[&[9], &[128], &[1], &[3], &[10]];
        for mask in 0..4096u16 {
            let mut forward = vec![vec![]; 5];
            let mut reverse = vec![vec![]; 5];
            let mut bit = 0;
            for (parent, children) in forward.iter_mut().enumerate().take(4) {
                for (child, parents) in reverse.iter_mut().enumerate().take(4) {
                    if parent == child {
                        continue;
                    }
                    if mask & (1 << bit) != 0 {
                        children.push(child);
                        parents.push(parent);
                    }
                    bit += 1;
                }
            }
            let mut paths = Vec::new();
            for root in [0, 1] {
                enumerate(&forward, &mut vec![root], 3, &mut paths);
            }
            let expected = paths
                .into_iter()
                .min_by(|a, b| a.iter().map(|id| keys[*id]).cmp(b.iter().map(|id| keys[*id])))
                .map(|mut path| {
                    path.push(4);
                    path
                });
            if mask % 2 == 0 {
                for parents in &mut reverse {
                    parents.reverse();
                }
            }
            assert_eq!(
                canonical_path(keys, &reverse, &[0, 1], 3, 4).expect("bounded graph"),
                expected,
                "mask {mask}"
            );
        }
    }

    #[test]
    fn synthetic_diamond_witness_uses_unsigned_keys_not_reverse_insertion_order() {
        let keys: &[&[u8]] = &[&[9], &[128], &[1], &[3], &[10]];
        for reverse in [
            vec![vec![], vec![0], vec![0], vec![1, 2], vec![]],
            vec![vec![], vec![0], vec![0], vec![2, 1], vec![]],
        ] {
            assert_eq!(
                canonical_path(keys, &reverse, &[0, 4], 3, 4).expect("bounded graph"),
                Some(vec![0, 2, 3, 4])
            );
        }
    }

    #[test]
    fn synthetic_same_key_cycles_do_not_loop_or_invent_an_expanding_origin() {
        let keys: &[&[u8]] = &[&[0], &[1], &[2], &[3], &[4]];
        let reverse = vec![vec![1], vec![0], vec![0], vec![2], vec![]];
        assert_eq!(
            canonical_path(keys, &reverse, &[0], 3, 4).expect("finite witness"),
            Some(vec![0, 2, 3, 4])
        );
        assert!(
            canonical_path(keys, &reverse, &[], 3, 4).expect("no nominal repetition").is_none()
        );
    }
}
