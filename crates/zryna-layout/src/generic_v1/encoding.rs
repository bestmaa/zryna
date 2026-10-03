//! Separately versioned successor records; original record payloads remain byte-compatible.

use super::{CanonicalMap, Failure, Record, raw, reserve};
use crate::{StorageTarget, VerifiedKind, push_u32, push_u64};
use sha2::{Digest, Sha256};

pub(super) fn document(target: StorageTarget, records: &[Record]) -> Result<Vec<u8>, Failure> {
    let mut bytes = reserve(128)?;
    bytes.extend_from_slice(b"ZRYNA-GENERIC-AGGREGATE-LAYOUT-V1\0");
    push_u32(&mut bytes, target.tag());
    push_u32(&mut bytes, u32::try_from(records.len()).map_err(|_| Failure::InternalFailure)?);
    for record in records {
        let payload = record_bytes(record)?;
        bytes
            .try_reserve(payload.len().checked_add(4).ok_or(Failure::InternalFailure)?)
            .map_err(|_| Failure::AllocationFailure)?;
        push_u32(&mut bytes, u32::try_from(payload.len()).map_err(|_| Failure::InternalFailure)?);
        bytes.extend_from_slice(&payload);
    }
    Ok(bytes)
}

fn record_bytes(record: &Record) -> Result<Vec<u8>, Failure> {
    if matches!(record.kind, raw::TypeKind::Base(_)) {
        return Ok(crate::encode_record(&record.physical));
    }
    let mut bytes = reserve(128)?;
    let tag = match record.kind {
        raw::TypeKind::Struct { .. } => 10,
        raw::TypeKind::Enum { .. } => 11,
        raw::TypeKind::Option { .. } => 12,
        raw::TypeKind::Result { .. } => 13,
        raw::TypeKind::Base(_) => return Err(Failure::InternalFailure),
    };
    push_u32(&mut bytes, tag);
    push_u32(&mut bytes, record.physical.id.index);
    push_u32(&mut bytes, record.physical.drop_kind);
    push_u32(&mut bytes, record.physical.runtime_kind);
    push_u64(&mut bytes, record.physical.size);
    push_u64(&mut bytes, record.physical.alignment);
    if let raw::TypeKind::Struct { module, declaration, .. }
    | raw::TypeKind::Enum { module, declaration, .. } = record.kind
    {
        push_u32(&mut bytes, module.0);
        push_u32(&mut bytes, declaration);
    }
    push_u32(
        &mut bytes,
        u32::try_from(record.arguments.len()).map_err(|_| Failure::InternalFailure)?,
    );
    for argument in &record.arguments {
        push_u32(&mut bytes, *argument);
    }
    match &record.physical.kind {
        VerifiedKind::Struct { fields, .. } => {
            bytes
                .try_reserve(fields.len().checked_mul(16).ok_or(Failure::InternalFailure)?)
                .map_err(|_| Failure::AllocationFailure)?;
            push_u32(
                &mut bytes,
                u32::try_from(fields.len()).map_err(|_| Failure::InternalFailure)?,
            );
            for field in fields {
                push_u32(&mut bytes, field.ordinal);
                push_u32(&mut bytes, field.ty.index);
                push_u64(&mut bytes, field.offset);
            }
        }
        VerifiedKind::Enum { variants, payload_offset, payload_size, .. } => {
            bytes
                .try_reserve(
                    variants
                        .len()
                        .checked_mul(8)
                        .and_then(|n| n.checked_add(20))
                        .ok_or(Failure::InternalFailure)?,
                )
                .map_err(|_| Failure::AllocationFailure)?;
            push_u32(
                &mut bytes,
                u32::try_from(variants.len()).map_err(|_| Failure::InternalFailure)?,
            );
            push_u64(&mut bytes, *payload_offset);
            push_u64(&mut bytes, *payload_size);
            for variant in variants {
                push_u32(&mut bytes, variant.ordinal);
                push_u32(&mut bytes, variant.payload.map_or(u32::MAX, |id| id.index));
            }
        }
        _ => return Err(Failure::InternalFailure),
    }
    Ok(bytes)
}

pub(super) fn identity(
    digest: &mut Sha256,
    kind: &raw::TypeKind,
    canonical: &CanonicalMap,
) -> Result<(), Failure> {
    let physical = match kind {
        raw::TypeKind::Struct { fields, .. } => Some((fields.as_slice(), None)),
        raw::TypeKind::Enum { variants, .. } => Some((&[][..], Some(variants.as_slice()))),
        raw::TypeKind::Base(crate::raw::TypeKind::Struct { fields, .. }) => {
            Some((fields.as_slice(), None))
        }
        raw::TypeKind::Base(crate::raw::TypeKind::Enum { variants, .. }) => {
            Some((&[][..], Some(variants.as_slice())))
        }
        _ => None,
    };
    if let Some((fields, variants)) = physical {
        let count = variants.map_or(fields.len(), <[raw::Variant]>::len);
        digest.update(u32::try_from(count).map_err(|_| Failure::InternalFailure)?.to_le_bytes());
        if let Some(variants) = variants {
            for variant in variants {
                digest.update(variant.ordinal.to_le_bytes());
                digest.update(
                    variant
                        .payload
                        .map_or(u32::MAX, |id| canonical.raw_to_index[id.0 as usize])
                        .to_le_bytes(),
                );
            }
        } else {
            for field in fields {
                digest.update(field.ordinal.to_le_bytes());
                digest.update(canonical.raw_to_index[field.ty.0 as usize].to_le_bytes());
            }
        }
    } else {
        digest.update(0u32.to_le_bytes());
    }
    Ok(())
}
