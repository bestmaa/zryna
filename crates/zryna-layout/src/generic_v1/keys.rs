//! Complete finite successor keys and canonical graph-local ID assignment.

use super::{CanonicalMap, Failure, TypeUniverseIdentity, raw, reject, reserve};
use crate::raw::TypeKind as Base;
use sha2::{Digest, Sha256};

pub(super) fn children(kind: &raw::TypeKind) -> Vec<usize> {
    match kind {
        raw::TypeKind::Struct { arguments, .. } | raw::TypeKind::Enum { arguments, .. } => {
            arguments.iter().map(|id| id.0 as usize).collect()
        }
        raw::TypeKind::Option { argument } => vec![argument.0 as usize],
        raw::TypeKind::Result { okay, error } => vec![okay.0 as usize, error.0 as usize],
        raw::TypeKind::Base(
            Base::FixedArray { element, .. }
            | Base::Vec { element }
            | Base::Shared { payload: element }
            | Base::Weak { payload: element },
        ) => vec![element.0 as usize],
        raw::TypeKind::Base(_) => vec![],
    }
}

pub(super) fn derive(graph: &raw::Graph) -> Result<Vec<Vec<u8>>, Failure> {
    let mut keys = reserve(graph.types.len())?;
    keys.resize_with(graph.types.len(), Vec::new);
    let mut states = reserve(graph.types.len())?;
    states.resize(graph.types.len(), 0u8);
    let mut depths = reserve(graph.types.len())?;
    depths.resize(graph.types.len(), 0u32);
    let mut stack = reserve(graph.types.len())?;
    for root in 0..graph.types.len() {
        if states[root] == 2 {
            continue;
        }
        states[root] = 1;
        stack.push((root, 0usize));
        while let Some((id, next)) = stack.last_mut() {
            let nodes = children(&graph.types[*id].kind);
            if *next < nodes.len() {
                let child = nodes[*next];
                *next += 1;
                if states[child] == 1 {
                    return Err(reject(
                        graph.types[*id].span,
                        "ZRYNA-L3003",
                        "closed argument/container key is recursive",
                    ));
                }
                if states[child] == 0 {
                    states[child] = 1;
                    stack.push((child, 0));
                }
                continue;
            }
            let (id, _) = stack.pop().ok_or(Failure::InternalFailure)?;
            let depth = nodes
                .iter()
                .map(|id| depths[*id])
                .max()
                .map_or(Some(0), |depth| depth.checked_add(1))
                .ok_or(Failure::InternalFailure)?;
            if depth > 64 {
                return Err(reject(
                    graph.types[id].span,
                    "ZRYNA-L7201",
                    "complete application depth exceeds 64",
                ));
            }
            let (tag, lanes) = prefix(&graph.types[id].kind)?;
            let size = nodes.iter().try_fold(
                1usize
                    .checked_add(lanes.len().checked_mul(4).ok_or(Failure::InternalFailure)?)
                    .ok_or(Failure::InternalFailure)?,
                |size, child| {
                    size.checked_add(4)
                        .and_then(|size| size.checked_add(keys[*child].len()))
                        .ok_or(Failure::InternalFailure)
                },
            )?;
            if size > 4096 {
                return Err(reject(
                    graph.types[id].span,
                    "ZRYNA-L7201",
                    "complete canonical key bytes exceed 4096",
                ));
            }
            let mut key = reserve(size)?;
            key.push(tag);
            for lane in lanes {
                key.extend_from_slice(&lane.to_le_bytes());
            }
            for child in nodes {
                key.extend_from_slice(
                    &u32::try_from(keys[child].len())
                        .map_err(|_| Failure::InternalFailure)?
                        .to_le_bytes(),
                );
                key.extend_from_slice(&keys[child]);
            }
            keys[id] = key;
            depths[id] = depth;
            states[id] = 2;
        }
    }
    Ok(keys)
}

fn prefix(kind: &raw::TypeKind) -> Result<(u8, Vec<u32>), Failure> {
    let nominal =
        |tag, module: raw::ModuleId, declaration, count| (tag, vec![module.0, declaration, count]);
    Ok(match kind {
        raw::TypeKind::Struct { module, declaration, arguments, .. } => nominal(
            0x12,
            *module,
            *declaration,
            u32::try_from(arguments.len()).map_err(|_| Failure::InternalFailure)?,
        ),
        raw::TypeKind::Enum { module, declaration, arguments, .. } => nominal(
            0x13,
            *module,
            *declaration,
            u32::try_from(arguments.len()).map_err(|_| Failure::InternalFailure)?,
        ),
        raw::TypeKind::Option { .. } => (0x14, vec![1]),
        raw::TypeKind::Result { .. } => (0x15, vec![2]),
        raw::TypeKind::Base(kind) => match kind {
            Base::Bool => (0, vec![]),
            Base::I32 => (1, vec![]),
            Base::String => (2, vec![]),
            Base::Struct { module, declaration, .. } => (0x10, vec![module.0, *declaration]),
            Base::Enum { module, declaration, .. } => (0x11, vec![module.0, *declaration]),
            Base::FixedArray { length, .. } => {
                (0x20, vec![u32::try_from(*length).map_err(|_| Failure::InternalFailure)?])
            }
            Base::Vec { .. } => (0x21, vec![]),
            Base::Shared { .. } => (0x22, vec![]),
            Base::Weak { .. } => (0x23, vec![]),
            Base::Borrow { .. } => return Err(Failure::InternalFailure),
        },
    })
}

pub(super) fn canonical(graph: &raw::Graph, keys: &[Vec<u8>]) -> Result<CanonicalMap, Failure> {
    let mut order = reserve(keys.len())?;
    order.extend(0..keys.len());
    order.sort_unstable_by(|a, b| keys[*a].cmp(&keys[*b]));
    if order.windows(2).any(|pair| keys[pair[0]] == keys[pair[1]]) {
        return Err(reject(None, "ZRYNA-L3001", "duplicate complete closed type key"));
    }
    if graph.types.iter().filter(|node| !matches!(node.kind, raw::TypeKind::Base(_))).count() > 4096
    {
        return Err(reject(None, "ZRYNA-L7201", "closed generic data count exceeds 4096"));
    }
    let mut raw_to_index = reserve(keys.len())?;
    raw_to_index.resize(keys.len(), 0u32);
    for (id, raw) in order.iter().copied().enumerate() {
        raw_to_index[raw] = u32::try_from(id).map_err(|_| Failure::InternalFailure)?;
    }
    let mut canonical =
        CanonicalMap { raw_to_index, type_to_raw: order, universe: TypeUniverseIdentity([0; 32]) };
    let mut digest = Sha256::new();
    digest.update(b"ZRYNA-GENERIC-TYPE-UNIVERSE-V1\0");
    for count in [graph.modules.len(), graph.declarations.len(), keys.len()] {
        digest.update(u32::try_from(count).map_err(|_| Failure::InternalFailure)?.to_le_bytes());
    }
    for module in &graph.modules {
        digest.update(module.id.0.to_le_bytes());
        digest.update(module.source_file.index().to_le_bytes());
        digest.update(module.data_declarations.to_le_bytes());
    }
    for declaration in &graph.declarations {
        digest.update(declaration.module.0.to_le_bytes());
        digest.update(declaration.index.to_le_bytes());
        digest.update([u8::from(declaration.kind == raw::NominalKind::Enum)]);
        digest.update(declaration.parameters.to_le_bytes());
        digest.update(declaration.members.to_le_bytes());
        digest.update(declaration.span.file().index().to_le_bytes());
        digest.update(declaration.span.start().to_le_bytes());
        digest.update(declaration.span.end().to_le_bytes());
    }
    for &raw_index in &canonical.type_to_raw {
        digest.update(
            u32::try_from(keys[raw_index].len())
                .map_err(|_| Failure::InternalFailure)?
                .to_le_bytes(),
        );
        digest.update(&keys[raw_index]);
        super::encoding::identity(&mut digest, &graph.types[raw_index].kind, &canonical)?;
    }
    let mut root_flags = reserve(keys.len())?;
    root_flags.resize(keys.len(), false);
    for id in &graph.program_roots {
        root_flags[canonical.raw_to_index[id.0 as usize] as usize] = true;
    }
    let mut roots = reserve(keys.len())?;
    for (index, present) in root_flags.into_iter().enumerate() {
        if present {
            roots.push(u32::try_from(index).map_err(|_| Failure::InternalFailure)?);
        }
    }
    digest.update(u32::try_from(roots.len()).map_err(|_| Failure::InternalFailure)?.to_le_bytes());
    for root in roots {
        digest.update(root.to_le_bytes());
    }
    canonical.universe = TypeUniverseIdentity(digest.finalize().into());
    Ok(canonical)
}
