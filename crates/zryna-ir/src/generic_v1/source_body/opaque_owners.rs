//! Conservative original-owner guard before any Copy specialization.
//!
//! Each potentially affine binding may have at most one whole-body reference. This deliberately
//! rejects some mutually exclusive/shadowed uses until full path-specific owner replay exists.
//! Closed Copy layouts never legalize duplication of an opaque original parameter.

use super::{Closed, Failure};
use crate::generic_v1::reject;
use zryna_syntax::v5::{RawExpressionKind, RawFunctionSyntax};

pub(super) fn check_binding(
    original: &RawFunctionSyntax,
    name: &str,
    ty: &Closed,
) -> Result<(), Failure> {
    let Closed::Stored(key) = ty else { return Ok(()) };
    if may_own(key, 0)?
        && original
            .body
            .expressions
            .iter()
            .filter(|expression| {
                matches!(&expression.kind, RawExpressionKind::Reference { name: reference }
                    if reference.text.eq_ignore_ascii_case(name))
            })
            .take(2)
            .count()
            > 1
    {
        return Err(reject(
            "original potentially owned binding requires full affine replay; Copy specialization cannot duplicate it",
        ));
    }
    Ok(())
}

fn may_own(key: &[u8], depth: usize) -> Result<bool, Failure> {
    if depth > 128 {
        return Err(reject("original owner type exceeds bounded source replay depth"));
    }
    let (offset, count) = match key.first() {
        Some(0 | 1) if key.len() == 1 => return Ok(false),
        Some(0x14 | 0x20) => (5usize, 1),
        Some(0x15) => (5usize, 2),
        // String, opaque slots, runtime handles and unproved nominal heads are affine.
        _ => return Ok(true),
    };
    let mut cursor = offset;
    let mut owned = false;
    for _ in 0..count {
        let length = u32::from_le_bytes(
            key.get(cursor..cursor + 4)
                .ok_or(Failure::InternalFailure)?
                .try_into()
                .map_err(|_| Failure::InternalFailure)?,
        ) as usize;
        cursor += 4;
        let end = cursor.checked_add(length).ok_or(Failure::InternalFailure)?;
        owned |= may_own(key.get(cursor..end).ok_or(Failure::InternalFailure)?, depth + 1)?;
        cursor = end;
    }
    if cursor != key.len() {
        return Err(Failure::InternalFailure);
    }
    Ok(owned)
}

#[cfg(test)]
mod tests {
    #[test]
    fn fixed_symbolic_keys_preserve_affinity_through_copy_shaped_containers() {
        // Fixed independent key bytes, never built by the substitution producer.
        for (key, owned) in [
            (&[0x14, 1, 0, 0, 0, 1, 0, 0, 0, 0x30][..], true),
            (&[0x14, 1, 0, 0, 0, 1, 0, 0, 0, 1][..], false),
            (&[0x15, 2, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 2][..], true),
            (&[0x20, 0, 0, 0, 0, 1, 0, 0, 0, 0x31][..], true),
            (&[0x20, 4, 0, 0, 0, 1, 0, 0, 0, 1][..], false),
            (&[0x21, 1, 0, 0, 0, 1][..], true),
        ] {
            assert_eq!(super::may_own(key, 0).expect("fixed symbolic key"), owned);
        }
    }
}
