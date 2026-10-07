//! Independently derived successor allocation metadata and exact target records.

use super::{ControlLayout, ElementLayout, Failure, raw, reject, reserve};
use crate::RuntimeAbiViolationKind;
use sha2::{Digest, Sha256};
use zryna_layout::{TypeCategory, generic_v1::VerifiedLayouts};

type Metadata = (Vec<ElementLayout>, Vec<ControlLayout>);

pub(super) fn derive(
    linear: &VerifiedLayouts,
    linux: &VerifiedLayouts,
) -> Result<Metadata, Failure> {
    let capacity = linear.types().len().checked_mul(2).ok_or(Failure::AllocationFailure)?;
    let mut elements = reserve(capacity)?;
    let mut controls = reserve(capacity)?;
    for layouts in [linear, linux] {
        let mut types = reserve(layouts.types().len())?;
        types.extend(layouts.types());
        let mut seen_elements = reserve(types.len())?;
        seen_elements.resize(types.len(), false);
        let mut seen_payloads = reserve(types.len())?;
        seen_payloads.resize(types.len(), false);
        for ty in &types {
            if !matches!(
                ty.category(),
                TypeCategory::Vec | TypeCategory::Shared | TypeCategory::Weak
            ) {
                continue;
            }
            let id =
                ty.referenced_type().ok_or_else(|| invalid("missing closed container child"))?;
            let index = usize::try_from(id.index()).map_err(|_| invalid("child index overflow"))?;
            let item = *types.get(index).ok_or_else(|| invalid("closed child outside universe"))?;
            if item.id() != id {
                return Err(invalid("closed child has a foreign compilation brand"));
            }
            if ty.category() == TypeCategory::Vec {
                if seen_elements[index] {
                    continue;
                }
                seen_elements[index] = true;
                if item.size() == 0 {
                    return Err(invalid("Vec element is zero-sized"));
                }
                let stride = crate::align_up(item.size(), item.alignment())
                    .ok_or_else(|| invalid("Vec stride overflows"))?;
                elements.push(ElementLayout {
                    target: layouts.target(),
                    element: id,
                    stride,
                    alignment: item.alignment(),
                });
            } else {
                if seen_payloads[index] {
                    continue;
                }
                seen_payloads[index] = true;
                let alignment = item.alignment().max(4);
                let payload_offset = crate::align_up(8, item.alignment())
                    .ok_or_else(|| invalid("control payload offset overflows"))?;
                let end = payload_offset
                    .checked_add(item.size())
                    .ok_or_else(|| invalid("control payload size overflows"))?;
                let size = crate::align_up(end, alignment)
                    .ok_or_else(|| invalid("control alignment overflows"))?;
                controls.push(ControlLayout {
                    target: layouts.target(),
                    payload: id,
                    payload_offset,
                    size,
                    alignment,
                });
            }
        }
    }
    Ok((elements, controls))
}

pub(super) fn records(
    linear: &VerifiedLayouts,
    linux: &VerifiedLayouts,
    controls: &[ControlLayout],
) -> Result<Vec<raw::RecordDeclaration>, Failure> {
    let mut records = reserve(controls.len().checked_add(6).ok_or(Failure::AllocationFailure)?)?;
    for (layouts, target, word) in [
        (linear, raw::RecordTarget::Linear32V1, 4_u64),
        (linux, raw::RecordTarget::LinuxX8664V1, 8_u64),
    ] {
        for kind in [raw::RecordKind::StringHandle, raw::RecordKind::VecHandle] {
            records.push(raw::RecordDeclaration {
                target,
                kind,
                size: word * 3,
                alignment: word,
                fields: vec![
                    raw::RecordField { role: raw::FieldRole::Pointer, offset: 0, size: word },
                    raw::RecordField { role: raw::FieldRole::Length, offset: word, size: word },
                    raw::RecordField {
                        role: raw::FieldRole::Capacity,
                        offset: word * 2,
                        size: word,
                    },
                ],
            });
        }
        records.push(raw::RecordDeclaration {
            target,
            kind: raw::RecordKind::BoolOutcome,
            size: 4,
            alignment: 4,
            fields: vec![raw::RecordField { role: raw::FieldRole::Bool, offset: 0, size: 4 }],
        });
        for control in controls.iter().filter(|control| control.target == layouts.target()) {
            let payload = layouts
                .type_by_id(control.payload)
                .ok_or_else(|| invalid("foreign control payload"))?;
            records.push(raw::RecordDeclaration {
                target,
                kind: raw::RecordKind::ControlBlock { payload_type: control.payload.index() },
                size: control.size,
                alignment: control.alignment,
                fields: vec![
                    raw::RecordField { role: raw::FieldRole::StrongCount, offset: 0, size: 4 },
                    raw::RecordField { role: raw::FieldRole::WeakCount, offset: 4, size: 4 },
                    raw::RecordField {
                        role: raw::FieldRole::Payload,
                        offset: control.payload_offset,
                        size: payload.size(),
                    },
                ],
            });
        }
    }
    Ok(records)
}

pub(super) fn hash(hash: &mut Sha256, elements: &[ElementLayout], controls: &[ControlLayout]) {
    for limit in [
        crate::MAX_DYNAMIC_ALLOCATION_BYTES,
        crate::MAX_STRING_BYTES,
        crate::MAX_VEC_ELEMENTS,
        crate::MAX_LIVE_ALLOCATIONS,
        crate::MAX_ALLOCATION_OPERATIONS,
        crate::MAX_STATUS_TRANSITIONS,
    ] {
        hash.update(limit.to_le_bytes());
    }
    hash.update((elements.len() as u64).to_le_bytes());
    for element in elements {
        hash.update([crate::target_tag(element.target)]);
        hash.update(element.element.index().to_le_bytes());
        hash.update(element.stride.to_le_bytes());
        hash.update(element.alignment.to_le_bytes());
    }
    hash.update((controls.len() as u64).to_le_bytes());
    for control in controls {
        hash.update([crate::target_tag(control.target)]);
        hash.update(control.payload.index().to_le_bytes());
        hash.update(control.payload_offset.to_le_bytes());
        hash.update(control.size.to_le_bytes());
        hash.update(control.alignment.to_le_bytes());
    }
}

fn invalid(message: &str) -> Failure {
    reject(RuntimeAbiViolationKind::Layout, message)
}
