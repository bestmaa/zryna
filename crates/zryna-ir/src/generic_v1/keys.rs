//! Independent, bounded decoding of complete canonical key bytes; no identity is issued.

use super::{Failure, budget, reject, reserve};

/// The three disjoint permitted root namespaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Domain {
    /// A stored closed type; function tags cannot occur anywhere inside it.
    Type,
    /// A closed generic function with one or two complete type arguments.
    FunctionInstance,
    /// A nongeneric source function root, without monomorphization arguments.
    SourceRoot,
}

/// Fully decoded root shape, separate from source declaration authentication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// Stored bool.
    Bool,
    /// Stored i32.
    I32,
    /// Owned String.
    String,
    /// Nongeneric nominal struct.
    Struct,
    /// Nongeneric nominal enum.
    Enum,
    /// Closed generic nominal struct.
    GenericStruct,
    /// Closed generic nominal enum.
    GenericEnum,
    /// Compiler-owned Option.
    Option,
    /// Compiler-owned Result.
    Result,
    /// Structural fixed array, with its exact length.
    FixedArray(u32),
    /// Structural Vec.
    Vec,
    /// Structural Shared.
    Shared,
    /// Structural Weak.
    Weak,
    /// Closed generic source function.
    FunctionInstance,
    /// Nongeneric source function root.
    SourceRoot,
}

/// Immutable parsed bytes. This is syntax of a key, not a sealed type/function identity.
#[derive(Debug)]
pub struct DecodedKey<'a> {
    kind: Kind,
    declaration: Option<(u32, u32)>,
    arguments: Vec<&'a [u8]>,
    depth: u32,
}

impl<'a> DecodedKey<'a> {
    /// Exact root kind.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.kind
    }
    /// Claimed original module and data/function index, absent for compiler families.
    #[must_use]
    pub const fn declaration(&self) -> Option<(u32, u32)> {
        self.declaration
    }
    /// Ordered complete argument/element bytes, validated independently of any producer.
    #[must_use]
    pub fn arguments(&self) -> impl ExactSizeIterator<Item = &'a [u8]> + '_ {
        self.arguments.iter().copied()
    }
    /// Greatest complete type application depth; function roots add no level.
    #[must_use]
    pub const fn application_depth(&self) -> u32 {
        self.depth
    }
}

struct Header<'a> {
    kind: Kind,
    declaration: Option<(u32, u32)>,
    children: Vec<&'a [u8]>,
    application: bool,
}

/// Decodes every byte of a complete key using an explicit bounded stack.
///
/// # Errors
/// Rejects wrong/unknown tags, hidden function keys, wrong arity, truncation, trailing bytes,
/// overflowing lengths and inherited array limits; depth/key budget failure is atomic.
pub fn decode(bytes: &[u8], domain: Domain) -> Result<DecodedKey<'_>, Failure> {
    if bytes.len() > 4096 {
        return Err(budget("complete canonical key byte limit 4096; first extra 4097"));
    }
    let root = header(bytes, domain)?;
    let mut depth = u32::from(root.application);
    let mut pending = reserve(66)?;
    for child in root.children.iter().rev() {
        pending.push((*child, depth));
    }
    while let Some((bytes, parent_depth)) = pending.pop() {
        let node = header(bytes, Domain::Type)?;
        let current = parent_depth
            .checked_add(u32::from(node.application))
            .ok_or(Failure::InternalFailure)?;
        if current > 64 {
            return Err(budget("complete type application depth limit 64; first extra 65"));
        }
        depth = depth.max(current);
        for child in node.children.iter().rev() {
            // Binary keys and depth <=64 bound this pending frontier by 65 nodes.
            pending.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
            pending.push((*child, current));
        }
    }
    Ok(DecodedKey {
        kind: root.kind,
        declaration: root.declaration,
        arguments: root.children,
        depth,
    })
}

fn header(bytes: &[u8], domain: Domain) -> Result<Header<'_>, Failure> {
    let Some(&tag) = bytes.first() else {
        return Err(reject("empty complete key"));
    };
    let permitted = match domain {
        Domain::Type => tag < 0x40,
        Domain::FunctionInstance => tag == 0x40,
        Domain::SourceRoot => tag == 0x41,
    };
    if !permitted {
        return Err(reject("function/type key namespace mismatch"));
    }
    let mut cursor = 1usize;
    let mut declaration = None;
    let mut count = 0u32;
    let mut application = false;
    let kind = match tag {
        0 => Kind::Bool,
        1 => Kind::I32,
        2 => Kind::String,
        0x10 | 0x11 | 0x12 | 0x13 | 0x40 | 0x41 => {
            declaration = Some((lane(bytes, &mut cursor)?, lane(bytes, &mut cursor)?));
            if matches!(tag, 0x12 | 0x13 | 0x40) {
                count = lane(bytes, &mut cursor)?;
                if !(1..=2).contains(&count) {
                    return Err(reject("generic nominal/function key arity is not one or two"));
                }
                application = tag != 0x40;
            }
            match tag {
                0x10 => Kind::Struct,
                0x11 => Kind::Enum,
                0x12 => Kind::GenericStruct,
                0x13 => Kind::GenericEnum,
                0x40 => Kind::FunctionInstance,
                0x41 => Kind::SourceRoot,
                _ => return Err(Failure::InternalFailure),
            }
        }
        0x14 | 0x15 => {
            count = lane(bytes, &mut cursor)?;
            let expected = if tag == 0x14 { 1 } else { 2 };
            if count != expected {
                return Err(reject("compiler enum key arity differs from its reserved family"));
            }
            application = true;
            if tag == 0x14 { Kind::Option } else { Kind::Result }
        }
        0x20 => {
            let length = lane(bytes, &mut cursor)?;
            if u64::from(length) > zryna_layout::MAX_ARRAY_LENGTH {
                return Err(reject("array length exceeds the inherited element ceiling"));
            }
            count = 1;
            application = true;
            Kind::FixedArray(length)
        }
        0x21..=0x23 => {
            count = 1;
            application = true;
            match tag {
                0x21 => Kind::Vec,
                0x22 => Kind::Shared,
                0x23 => Kind::Weak,
                _ => return Err(Failure::InternalFailure),
            }
        }
        _ => return Err(reject("unknown, borrow or unresolved complete key tag")),
    };
    let mut children = reserve(usize::try_from(count).map_err(|_| Failure::InternalFailure)?)?;
    for _ in 0..count {
        let length =
            usize::try_from(lane(bytes, &mut cursor)?).map_err(|_| Failure::InternalFailure)?;
        let end = cursor
            .checked_add(length)
            .ok_or_else(|| reject("overflowing complete child key length"))?;
        let child = bytes
            .get(cursor..end)
            .filter(|child| !child.is_empty())
            .ok_or_else(|| reject("truncated, empty or overflowing complete child key"))?;
        children.push(child);
        cursor = end;
    }
    if cursor != bytes.len() {
        return Err(reject("trailing key bytes or invented child lane"));
    }
    Ok(Header { kind, declaration, children, application })
}

fn lane(bytes: &[u8], cursor: &mut usize) -> Result<u32, Failure> {
    let end = cursor.checked_add(4).ok_or(Failure::InternalFailure)?;
    let lane = bytes
        .get(*cursor..end)
        .ok_or_else(|| reject("truncated unsigned little-endian key lane"))?
        .try_into()
        .map_err(|_| Failure::InternalFailure)?;
    *cursor = end;
    Ok(u32::from_le_bytes(lane))
}

#[cfg(test)]
mod tests;
