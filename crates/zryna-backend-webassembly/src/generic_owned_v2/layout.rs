//! Checked private scalar-lane and function-local allocation from retained layout authority.

use super::{MAX_FUNCTION_LOCALS, MAX_TYPE_LANES, budget, invariant};
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::{owned_v2::VerifiedOwnedProgram, raw};
use zryna_layout::{StorageTarget, TypeCategory, generic_v1::TypeView};

pub(super) struct Layout<'p, 'a> {
    pub(super) program: &'p VerifiedOwnedProgram<'a>,
    pub(super) functions: Vec<Locals>,
    pub(super) return_lanes: u32,
    widths: Vec<Option<u32>>,
    pub(super) literals: std::collections::BTreeMap<(usize, u32), (u32, Vec<u8>)>,
}

pub(super) struct Locals {
    pub(super) parameters: u32,
    pub(super) values: Vec<Value>,
    pub(super) scratch: u32,
    pub(super) state: u32,
    pub(super) declared: u32,
    pub(super) status: u32,
    pub(super) frame: u32,
}

#[derive(Clone, Copy)]
pub(super) struct Value {
    pub(super) start: u32,
    pub(super) width: u32,
}

impl<'p, 'a> Layout<'p, 'a> {
    pub(super) fn new(program: &'p VerifiedOwnedProgram<'a>) -> Result<Self, Diagnostic> {
        let views = program.layouts(StorageTarget::Linear32V1).types();
        let mut types = Vec::new();
        types.try_reserve(views.len()).map_err(|_| budget())?;
        types.extend(views);
        let mut widths = Vec::new();
        widths.try_reserve(types.len()).map_err(|_| budget())?;
        widths.resize(types.len(), None);
        let mut functions = Vec::new();
        functions.try_reserve(program.functions().len()).map_err(|_| budget())?;
        let mut return_lanes = 0;
        for function in program.functions() {
            let result = width(function.result, &types, &mut widths, 0)?;
            return_lanes = return_lanes.max(result);
            let mut parameters = 0;
            for ty in &function.parameters {
                parameters = add(parameters, width(*ty, &types, &mut widths, 0)?)?;
            }
            let count = function.blocks.iter().try_fold(0usize, |count, block| {
                count
                    .checked_add(block.parameters.len() + block.instructions.len())
                    .ok_or_else(budget)
            })?;
            let mut values = Vec::new();
            values.try_reserve(count).map_err(|_| budget())?;
            values.resize(count, Value { start: 0, width: 0 });
            let mut next = parameters;
            let mut scratch_width = 0;
            for block in &function.blocks {
                let block_width = block.parameters.iter().try_fold(0, |sum, parameter| {
                    add(sum, width(parameter.ty, &types, &mut widths, 0)?)
                })?;
                for definition in
                    block.parameters.iter().chain(block.instructions.iter().map(|i| &i.result))
                {
                    let lanes = width(definition.ty, &types, &mut widths, 0)?;
                    values[definition.id as usize] = Value { start: next, width: lanes };
                    next = add(next, lanes)?;
                }
                scratch_width = scratch_width.max(block_width);
            }
            let scratch = next;
            let state = add(scratch, scratch_width)?;
            let status = add(state, 1)?;
            let frame = add(status, 1)?;
            let end = add(frame, 1)?;
            functions.push(Locals {
                parameters,
                values,
                scratch,
                state,
                status,
                frame,
                declared: end - parameters,
            });
        }
        let mut literals = std::collections::BTreeMap::new();
        let mut cursor = 4096u32;
        for (fi, f) in program.functions().iter().enumerate() {
            for block in &f.blocks {
                for i in &block.instructions {
                    if let Some(zryna_ir::generic_v1::owned_v2::raw::Operation::StringLiteral(
                        bytes,
                    )) = program.extension(fi, i.result.id)
                    {
                        let n = u32::try_from(bytes.len()).map_err(|_| budget())?;
                        let end = cursor
                            .checked_add(n)
                            .filter(|n| *n <= 32 * 1024 * 1024)
                            .ok_or_else(budget)?;
                        literals.insert((fi, i.result.id), (cursor, bytes.clone()));
                        cursor = end;
                    }
                }
            }
        }
        Ok(Self { program, functions, return_lanes, widths, literals })
    }

    pub(super) fn width(&self, ty: raw::Type) -> Result<u32, Diagnostic> {
        match ty {
            raw::Type::Unit => Ok(0),
            raw::Type::Stored(index) => {
                self.widths.get(index as usize).copied().flatten().ok_or_else(invariant)
            }
            raw::Type::Borrow { referent, .. } => self.width(raw::Type::Stored(referent)),
        }
    }

    pub(super) fn source_call(&self, module: u32, function: u32) -> Result<u32, Diagnostic> {
        let mut key = [0u8; 9];
        key[0] = 0x41;
        key[1..5].copy_from_slice(&module.to_le_bytes());
        key[5..9].copy_from_slice(&function.to_le_bytes());
        self.program
            .functions()
            .binary_search_by(|f| f.key.as_slice().cmp(&key))
            .map_err(|_| invariant())
            .and_then(|index| u32::try_from(index).map_err(|_| budget()))
    }
}

fn add(left: u32, right: u32) -> Result<u32, Diagnostic> {
    left.checked_add(right).filter(|n| *n <= MAX_FUNCTION_LOCALS).ok_or_else(budget)
}

fn width(
    ty: raw::Type,
    types: &[TypeView<'_>],
    cache: &mut [Option<u32>],
    depth: usize,
) -> Result<u32, Diagnostic> {
    if let raw::Type::Borrow { referent, .. } = ty {
        return width(raw::Type::Stored(referent), types, cache, depth);
    }
    let raw::Type::Stored(index) = ty else {
        return if ty == raw::Type::Unit { Ok(0) } else { Err(invariant()) };
    };
    if let Some(n) = cache.get(index as usize).copied().flatten() {
        return Ok(n);
    }
    if depth > zryna_layout::MAX_TRAVERSAL_DEPTH {
        return Err(budget());
    }
    let ty = types.get(index as usize).copied().ok_or_else(invariant)?;
    let mut child = |id: zryna_layout::generic_v1::TypeId| {
        width(raw::Type::Stored(id.index()), types, cache, depth + 1)
    };
    let n = match ty.category() {
        TypeCategory::Bool | TypeCategory::I32 => 1,
        TypeCategory::String => 3,
        TypeCategory::Enum => {
            let mut maximum = 0;
            for (_, payload) in ty.variants() {
                if let Some(payload) = payload {
                    maximum = maximum.max(child(payload)?);
                }
            }
            maximum.checked_add(1).ok_or_else(budget)?
        }
        TypeCategory::Struct => {
            let mut total = 0u32;
            for (_, field, _) in ty.fields() {
                total = total.checked_add(child(field)?).ok_or_else(budget)?;
            }
            total
        }
        TypeCategory::FixedArray => {
            let element = ty.referenced_type().ok_or_else(invariant)?;
            let lanes = child(element)?;
            let physical = types.get(element.index() as usize).ok_or_else(invariant)?;
            let stride =
                physical.size().checked_add(physical.alignment() - 1).ok_or_else(budget)?
                    / physical.alignment()
                    * physical.alignment();
            if lanes == 0 {
                0
            } else {
                if stride == 0 || ty.size() % stride != 0 {
                    return Err(invariant());
                }
                let length = u32::try_from(ty.size() / stride).map_err(|_| budget())?;
                lanes.checked_mul(length).ok_or_else(budget)?
            }
        }
        _ => return Err(invariant()),
    };
    if n > MAX_TYPE_LANES {
        return Err(budget());
    }
    cache[index as usize] = Some(n);
    Ok(n)
}
