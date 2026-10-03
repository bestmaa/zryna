use super::{InstantiationFailure, MAX_KEY_BYTES, reserve};

pub(super) fn encode(
    tag: u8,
    lanes: &[u32],
    children: &[&[u8]],
) -> Result<(Vec<u8>, usize), InstantiationFailure> {
    let size = children.iter().try_fold(
        1usize
            .checked_add(lanes.len().checked_mul(4).ok_or(InstantiationFailure::InternalFailure)?)
            .ok_or(InstantiationFailure::InternalFailure)?,
        |size, child| {
            size.checked_add(4)
                .and_then(|size| size.checked_add(child.len()))
                .ok_or(InstantiationFailure::InternalFailure)
        },
    )?;
    // The caller supplies the authenticated location for a key-size failure.
    if size > MAX_KEY_BYTES {
        return Ok((Vec::new(), size));
    }
    let mut key = reserve(size)?;
    key.push(tag);
    for lane in lanes {
        key.extend_from_slice(&lane.to_le_bytes());
    }
    for child in children {
        key.extend_from_slice(
            &u32::try_from(child.len())
                .map_err(|_| InstantiationFailure::InternalFailure)?
                .to_le_bytes(),
        );
        key.extend_from_slice(child);
    }
    Ok((key, size))
}
