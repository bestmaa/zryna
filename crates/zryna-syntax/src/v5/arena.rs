//! Exact occurrence forests, independent of source spelling and semantic identity.

use super::DeclarationError;

pub(super) fn malformed() -> DeclarationError {
    DeclarationError::malformed(None)
}

pub(super) fn budget() -> DeclarationError {
    DeclarationError { code: "ZRYNA-Y5201", span: None }
}

pub(super) fn own(owners: &mut [bool], id: u32) -> Result<usize, DeclarationError> {
    let index = usize::try_from(id).map_err(|_| malformed())?;
    let owned = owners.get_mut(index).ok_or_else(malformed)?;
    if *owned {
        return Err(malformed());
    }
    *owned = true;
    Ok(index)
}

pub(super) fn forest(
    count: usize,
    roots: &[u32],
    mut children: impl FnMut(usize) -> Vec<u32>,
) -> Result<u32, DeclarationError> {
    let mut owners = vec![false; count];
    let mut next = 0;
    let mut maximum_depth = 0;
    for root in roots {
        let mut stack = vec![(*root, false, 1u32)];
        while let Some((id, leaving, depth)) = stack.pop() {
            if depth > crate::v4::MAX_NESTING_DEPTH {
                return Err(budget());
            }
            maximum_depth = maximum_depth.max(depth);
            if leaving {
                if id as usize != next {
                    return Err(malformed());
                }
                next += 1;
                continue;
            }
            let index = own(&mut owners, id)?;
            let child_ids = children(index);
            if child_ids.iter().any(|child| *child >= id) {
                return Err(malformed());
            }
            stack.push((id, true, depth));
            for child in child_ids.into_iter().rev() {
                stack.push((child, false, depth.checked_add(1).ok_or_else(budget)?));
            }
        }
    }
    if next != count || owners.iter().any(|owned| !owned) {
        return Err(malformed());
    }
    Ok(maximum_depth)
}
