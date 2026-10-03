//! First executable successor lane: immutable Copy values and exact enum/call source replay.
//!
//! Owned values, loans, mutable state and their cleanup/fault plans remain rejected. This
//! authority does not activate a driver profile or convert into any existing M3 program.

use super::{Failure, raw, reject, reserve, source::Originals, wire::DecodedProgram};
use zryna_layout::{
    StorageTarget,
    generic_v1::{TypeId, VerifiedLayouts},
};
use zryna_ownership_runtime_abi::generic_v1::VerifiedOwnershipRuntimeAbi;
use zryna_source::{FileId, SourceMap};
use zryna_syntax::v5::VerifiedProjectSyntaxV5;

mod producer;
#[cfg(test)]
mod tests;
mod type_demand;

pub use producer::produce_claim;

/// Private canonical function identity bound to its exact compilation, entry and universe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FunctionId {
    brand: TypeId,
    entry: FileId,
    index: usize,
}

/// Backend-consumable Copy-only program, issued only after complete wire/source/graph checks.
///
/// ```compile_fail
/// fn fake(raw: zryna_ir::generic_v1::raw::Program) {
///     let _: zryna_ir::generic_v1::copy_v1::VerifiedCopyProgram<'_> = raw;
/// }
/// ```
#[derive(Debug)]
pub struct VerifiedCopyProgram<'a> {
    program: raw::Program,
    brand: TypeId,
    linear: &'a VerifiedLayouts,
    linux: &'a VerifiedLayouts,
    runtime: &'a VerifiedOwnershipRuntimeAbi<'a>,
    abi: zryna_abi::VerifiedScalarAbiModule,
    exports: Vec<usize>,
    entry: FileId,
}

impl<'a> VerifiedCopyProgram<'a> {
    /// Issues the exact function identity for one verified canonical index.
    #[must_use]
    pub fn function_id(&self, index: usize) -> Option<FunctionId> {
        self.program.functions.get(index).map(|_| FunctionId {
            brand: self.brand,
            entry: self.entry,
            index,
        })
    }
    /// Looks up only a function identity belonging to this exact compilation and entry.
    #[must_use]
    pub fn function(&self, id: FunctionId) -> Option<&raw::Function> {
        if id.brand != self.brand || id.entry != self.entry {
            return None;
        }
        self.program.functions.get(id.index)
    }
    /// Immutable functions whose complete bodies and demand were authenticated.
    #[must_use]
    pub fn functions(&self) -> &[raw::Function] {
        &self.program.functions
    }
    /// Exact source/universe-bound primitive identity retained by this program.
    #[must_use]
    pub const fn compilation_brand(&self) -> TypeId {
        self.brand
    }
    /// Selected retained successor layout authority.
    #[must_use]
    pub const fn layouts(&self, target: StorageTarget) -> &'a VerifiedLayouts {
        match target {
            StorageTarget::Linear32V1 => self.linear,
            StorageTarget::LinuxX8664V1 => self.linux,
        }
    }
    /// Exact separate runtime declaration authority. This lane performs no allocation.
    #[must_use]
    pub const fn runtime(&self) -> &'a VerifiedOwnershipRuntimeAbi<'a> {
        self.runtime
    }
    /// Separately sealed scalar ABI for nongeneric exports of the exact selected entry file.
    #[must_use]
    pub const fn scalar_abi(&self) -> &zryna_abi::VerifiedScalarAbiModule {
        &self.abi
    }
    /// Canonical function indices corresponding to scalar ABI declaration order.
    #[must_use]
    pub fn export_functions(&self) -> &[usize] {
        &self.exports
    }
    /// Layout-derived ownership proof: every used stored value is Copy, with zero loans/drops.
    #[must_use]
    pub const fn ownership_effects(&self) -> (usize, usize) {
        (0, 0)
    }
}

/// Independently seals one complete wire-admitted Copy program against exact source authorities.
///
/// # Errors
/// Rejects foreign runtime/source/entry identity, fabricated/omitted/reordered source operations,
/// opaque-template specialization, extra demanded instances, owned values, loans and public
/// aggregates. All existing M3 constructors remain separate.
pub fn verify<'a>(
    decoded: DecodedProgram,
    syntax: &VerifiedProjectSyntaxV5,
    sources: &SourceMap,
    entry: FileId,
    linear: &'a VerifiedLayouts,
    linux: &'a VerifiedLayouts,
    runtime: &'a VerifiedOwnershipRuntimeAbi<'a>,
) -> Result<VerifiedCopyProgram<'a>, Failure> {
    if sources.verify_file_id(entry.index()).map_err(|_| reject("unknown successor entry file"))?
        != entry
        || !runtime.is_bound_to(linear, linux)
    {
        return Err(reject("successor entry/runtime issuer belongs to a foreign compilation"));
    }
    let program = decoded.0;
    super::validate_source_graph(&program, syntax, sources, linear, linux)?;
    let originals = Originals::check(&program, syntax)?;
    entry_closure(&originals, entry.index())?;
    super::source_body::check(&program, &originals)?;
    type_demand::check(&program, linear)?;
    copy_ownership(&program, linear)?;
    let (abi, exports) = scalar_abi(&program, entry.index())?;
    let brand = linear.types().next().ok_or(Failure::InternalFailure)?.id();
    Ok(VerifiedCopyProgram { program, brand, linear, linux, runtime, abi, exports, entry })
}

fn entry_closure(originals: &Originals<'_>, entry: u32) -> Result<(), Failure> {
    let mut reached = reserve(originals.units.len())?;
    reached.resize(originals.units.len(), false);
    let mut pending = reserve(originals.units.len())?;
    reached[entry as usize] = true;
    pending.push(entry);
    while let Some(module) = pending.pop() {
        for import in &originals.units[module as usize].imports {
            for binding in &import.bindings {
                let (super::source::Target::Data(target, _)
                | super::source::Target::Function(target, _)) =
                    originals.resolve(module, &binding.local.text)?;
                if !reached[target as usize] {
                    reached[target as usize] = true;
                    pending.push(target);
                }
            }
        }
    }
    if reached.iter().any(|seen| !seen) {
        return Err(reject(
            "source map contains a module outside the exact successor entry closure",
        ));
    }
    Ok(())
}

fn copy_ownership(program: &raw::Program, layouts: &VerifiedLayouts) -> Result<(), Failure> {
    let mut types = reserve(layouts.types().len())?;
    types.extend(layouts.types());
    let check = |ty: raw::Type| match ty {
        raw::Type::Unit => Ok(()),
        raw::Type::Stored(id)
            if types
                .get(id as usize)
                .is_some_and(|ty| ty.drop_kind() == 0 && ty.runtime_kind() == 0) =>
        {
            Ok(())
        }
        _ => {
            Err(reject("executable Copy lane requires an empty owned/loan/drop/runtime effect set"))
        }
    };
    for function in &program.functions {
        for parameter in &function.parameters {
            check(*parameter)?;
        }
        check(function.result)?;
        for block in &function.blocks {
            for parameter in &block.parameters {
                check(parameter.ty)?;
            }
            for instruction in &block.instructions {
                check(instruction.result.ty)?;
            }
            if matches!(
                block.terminator,
                raw::Terminator::ClosedEnumMatch {
                    mode: raw::MatchMode::SharedBorrow | raw::MatchMode::ExclusiveBorrow,
                    ..
                }
            ) {
                return Err(reject("Copy executable match cannot carry loan transfers"));
            }
        }
    }
    Ok(())
}

fn scalar_abi(
    program: &raw::Program,
    entry: u32,
) -> Result<(zryna_abi::VerifiedScalarAbiModule, Vec<usize>), Failure> {
    let scalar = |ty| match ty {
        raw::Type::Stored(0) => Ok(zryna_abi::raw::Type::Bool),
        raw::Type::Stored(1) => Ok(zryna_abi::raw::Type::I32),
        _ => Err(reject("generic aggregate or unit cannot enter scalar ABI v1")),
    };
    let mut claims = reserve(program.functions.len())?;
    let mut indices = reserve(program.functions.len())?;
    for declaration in program
        .declarations
        .iter()
        .filter(|declaration| declaration.module == entry && declaration.parameters == 0)
    {
        let mut key = [0u8; 9];
        key[0] = 0x41;
        key[1..5].copy_from_slice(&entry.to_le_bytes());
        key[5..9].copy_from_slice(&declaration.function.to_le_bytes());
        let index = program
            .functions
            .binary_search_by(|function| function.key.as_slice().cmp(&key))
            .map_err(|_| Failure::InternalFailure)?;
        let function = &program.functions[index];
        if let Some(name) = &function.public_export {
            let mut parameters = reserve(function.parameters.len())?;
            for parameter in &function.parameters {
                parameters.push(scalar(*parameter)?);
            }
            claims.push(zryna_abi::raw::Export::new(
                name.clone(),
                zryna_abi::raw::Signature::new(parameters, scalar(function.result)?),
            ));
            indices.push(index);
        }
    }
    let abi = zryna_abi::verify_v1(zryna_abi::raw::Module::new(claims))
        .map_err(|_| reject("successor public exports violate the unchanged scalar ABI v1"))?;
    Ok((abi, indices))
}
