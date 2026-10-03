//! Bounded source-type substitution, independent of semantic producer tables.

use super::{
    Failure, budget, keys, raw, reject, reserve,
    source::{Originals, Target},
};
use zryna_syntax::v5::{RawDataDeclarationKind, RawTypeParameterList, RawTypeSyntaxKind};

pub(super) enum Closed {
    Stored(Vec<u8>),
    Unit,
    Borrow(Vec<u8>, bool),
}

enum Shape {
    Key(u8, Vec<u32>, Vec<u32>),
    Borrow(u32, bool),
}

enum Task {
    Node(u32),
    Finish(Shape),
}

pub(super) struct Resolver<'a, 'b> {
    pub originals: &'a Originals<'b>,
    pub module: u32,
    pub parameters: Option<&'b RawTypeParameterList>,
    pub arguments: Vec<&'a [u8]>,
}

impl Resolver<'_, '_> {
    pub fn resolve(&self, occurrence: u32) -> Result<Closed, Failure> {
        let mut pending = reserve(257)?;
        let mut values = reserve(129)?;
        pending.push(Task::Node(occurrence));
        while let Some(task) = pending.pop() {
            match task {
                Task::Node(index) => {
                    let kind = &self.originals.units[self.module as usize]
                        .type_syntax
                        .get(index as usize)
                        .ok_or_else(|| reject("unknown source type occurrence"))?
                        .kind;
                    match kind {
                        RawTypeSyntaxKind::Named { name } => values.push(self.named(&name.text)?),
                        RawTypeSyntaxKind::String { .. } => {
                            values.push(Closed::Stored(copy(&[2])?));
                        }
                        RawTypeSyntaxKind::Missing => {
                            return Err(reject("missing original signature/member type"));
                        }
                        _ => {
                            let shape = self.shape(kind)?;
                            let children = match &shape {
                                Shape::Key(_, _, children) => children.as_slice(),
                                Shape::Borrow(child, _) => std::slice::from_ref(child),
                            };
                            pending
                                .try_reserve(children.len() + 1)
                                .map_err(|_| Failure::AllocationFailure)?;
                            for child in children {
                                if *child >= index {
                                    return Err(reject(
                                        "source type forest has a forward/cyclic occurrence",
                                    ));
                                }
                            }
                            let children = copy_ids(children)?;
                            pending.push(Task::Finish(shape));
                            pending.extend(children.into_iter().rev().map(Task::Node));
                        }
                    }
                }
                Task::Finish(shape) => {
                    let count = match &shape {
                        Shape::Key(_, _, children) => children.len(),
                        Shape::Borrow(..) => 1,
                    };
                    let start = values.len().checked_sub(count).ok_or(Failure::InternalFailure)?;
                    let mut children = reserve(count)?;
                    children.extend(values.drain(start..));
                    let mut stored = reserve(count)?;
                    for child in children {
                        let Closed::Stored(key) = child else {
                            return Err(reject(
                                "unit/borrow is not a stored child or ZrynaValue argument",
                            ));
                        };
                        stored.push(key);
                    }
                    values.push(match shape {
                        Shape::Key(tag, lanes, _) => Closed::Stored(encode(tag, &lanes, &stored)?),
                        Shape::Borrow(_, exclusive) => Closed::Borrow(stored.remove(0), exclusive),
                    });
                }
            }
        }
        if values.len() != 1 {
            return Err(Failure::InternalFailure);
        }
        values.pop().ok_or(Failure::InternalFailure)
    }

    fn named(&self, name: &str) -> Result<Closed, Failure> {
        if let Some(index) = self.parameters.and_then(|list| {
            list.parameters.iter().position(|parameter| parameter.name.text == name)
        }) {
            let argument = self
                .arguments
                .get(index)
                .ok_or_else(|| reject("unbound original type parameter"))?;
            return Ok(Closed::Stored(copy(argument)?));
        }
        match name {
            "bool" => Ok(Closed::Stored(copy(&[0])?)),
            "i32" => Ok(Closed::Stored(copy(&[1])?)),
            "unit" => Ok(Closed::Unit),
            "Option" | "Result" => {
                Err(reject("compiler generic family requires explicit arguments"))
            }
            _ => {
                let (tag, lanes, arity) = self.nominal(name)?;
                if arity != 0 {
                    return Err(reject("source generic nominal requires exact explicit arguments"));
                }
                Ok(Closed::Stored(encode(tag, &lanes, &[])?))
            }
        }
    }

    fn nominal(&self, name: &str) -> Result<(u8, Vec<u32>, usize), Failure> {
        let Target::Data(module, index) = self.originals.resolve(self.module, name)? else {
            return Err(reject("type name denotes an original function"));
        };
        let data = &self.originals.units[module as usize].data_declarations[index as usize];
        let count = data.type_parameters.as_ref().map_or(0, |list| list.parameters.len());
        let tag = match (&data.kind, count) {
            (RawDataDeclarationKind::Struct { .. }, 0) => 0x10,
            (RawDataDeclarationKind::Enum { .. }, 0) => 0x11,
            (RawDataDeclarationKind::Struct { .. }, _) => 0x12,
            (RawDataDeclarationKind::Enum { .. }, _) => 0x13,
        };
        let mut lanes = reserve(3)?;
        lanes.extend([module, index]);
        if count != 0 {
            lanes.push(u32::try_from(count).map_err(|_| Failure::InternalFailure)?);
        }
        Ok((tag, lanes, count))
    }

    fn shape(&self, kind: &RawTypeSyntaxKind) -> Result<Shape, Failure> {
        let (tag, lane, child) = match kind {
            RawTypeSyntaxKind::Vec { argument, .. } => (0x21, None, *argument),
            RawTypeSyntaxKind::Shared { argument, .. } => (0x22, None, *argument),
            RawTypeSyntaxKind::Weak { argument, .. } => (0x23, None, *argument),
            RawTypeSyntaxKind::FixedArray { element, length, .. } => {
                (0x20, Some(*length), *element)
            }
            RawTypeSyntaxKind::Borrow { argument, .. } => {
                return Ok(Shape::Borrow(*argument, false));
            }
            RawTypeSyntaxKind::BorrowMut { argument, .. } => {
                return Ok(Shape::Borrow(*argument, true));
            }
            RawTypeSyntaxKind::Application { name, type_arguments } => {
                if self.parameters.is_some_and(|list| {
                    list.parameters.iter().any(|parameter| parameter.name.text == name.text)
                }) {
                    return Err(reject(
                        "opaque parameter cannot be a higher-kinded application head",
                    ));
                }
                let (tag, lanes, arity) = match name.text.as_str() {
                    "Option" => (0x14, copy_ids(&[1])?, 1),
                    "Result" => (0x15, copy_ids(&[2])?, 2),
                    _ => self.nominal(&name.text)?,
                };
                if arity == 0 || type_arguments.arguments.len() != arity {
                    return Err(reject("source application arity differs from original/family"));
                }
                return Ok(Shape::Key(tag, lanes, copy_ids(&type_arguments.arguments)?));
            }
            _ => return Err(Failure::InternalFailure),
        };
        let mut lanes = reserve(usize::from(lane.is_some()))?;
        lanes.extend(lane);
        Ok(Shape::Key(tag, lanes, copy_ids(&[child])?))
    }
}

pub(super) fn type_id(program: &raw::Program, closed: Closed) -> Result<raw::Type, Failure> {
    let stored = |key: &[u8]| {
        program
            .type_keys
            .binary_search_by(|candidate| candidate.as_slice().cmp(key))
            .map_err(|_| reject("substituted original type is absent from the complete universe"))
            .and_then(|index| u32::try_from(index).map_err(|_| Failure::InternalFailure))
    };
    Ok(match closed {
        Closed::Stored(key) => raw::Type::Stored(stored(&key)?),
        Closed::Unit => raw::Type::Unit,
        Closed::Borrow(key, exclusive) => raw::Type::Borrow { referent: stored(&key)?, exclusive },
    })
}

fn copy(bytes: &[u8]) -> Result<Vec<u8>, Failure> {
    let mut result = reserve(bytes.len())?;
    result.extend_from_slice(bytes);
    Ok(result)
}

fn copy_ids(ids: &[u32]) -> Result<Vec<u32>, Failure> {
    let mut result = reserve(ids.len())?;
    result.extend_from_slice(ids);
    Ok(result)
}

fn encode(tag: u8, lanes: &[u32], children: &[Vec<u8>]) -> Result<Vec<u8>, Failure> {
    let length = children.iter().try_fold(1 + 4 * lanes.len(), |total, child| {
        total
            .checked_add(4)
            .and_then(|total| total.checked_add(child.len()))
            .ok_or(Failure::InternalFailure)
    })?;
    if length > 4096 {
        return Err(budget("substituted complete type key exceeds 4096 bytes"));
    }
    let mut key = reserve(length)?;
    key.push(tag);
    for lane in lanes {
        key.extend_from_slice(&lane.to_le_bytes());
    }
    for child in children {
        key.extend_from_slice(
            &u32::try_from(child.len()).map_err(|_| Failure::InternalFailure)?.to_le_bytes(),
        );
        key.extend_from_slice(child);
    }
    keys::decode(&key, keys::Domain::Type)?;
    Ok(key)
}
