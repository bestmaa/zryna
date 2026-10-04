//! Complete original affine source replay, independent of the raw cleanup plan.
//!
//! Unsupported source operations fail closed, including in unused original templates.
//! The symbolic pass retains opaque parameter keys; a closed i32 instance cannot legalize T + T.

use crate::generic_v1::{
    Failure, keys, raw, reject, reserve,
    source::Originals,
    source_types::{Closed, Resolver, type_id},
};
use zryna_syntax::{v4::RawStatementKind, v5::RawFunctionSyntax};

mod enums;
mod expressions;
mod normalize;
mod owners;
mod resources;
mod statements;
use super::raw::{Extension, Operation as Owned};
use resources::Budget;
use std::collections::BTreeMap;

#[derive(Clone)]
struct Value {
    id: u32,
    ty: Closed,
}

#[derive(Clone)]
struct Binding<'a> {
    name: &'a str,
    value: Value,
    mutable: bool,
}

struct Builder<'a, 'b> {
    program: &'a raw::Program,
    sources: &'a zryna_source::SourceMap,
    resolver: Resolver<'a, 'b>,
    original: &'b RawFunctionSyntax,
    symbolic: bool,
    blocks: Vec<raw::Block>,
    block: usize,
    next: u32,
    scope_start: usize,
    locals: Vec<Binding<'b>>,
    result: Closed,
    extensions: Vec<Extension>,
    affine: Vec<bool>,
    alive: Vec<bool>,
    loans: BTreeMap<u32, (u32, bool)>,
    loan_parents: BTreeMap<u32, u32>,
    budget: Budget,
}

pub(super) fn check(
    claim: &super::raw::Program,
    originals: &Originals<'_>,
    sources: &zryna_source::SourceMap,
) -> Result<(), Failure> {
    let program = &claim.graph;
    if claim.extensions.len() != program.functions.len() {
        return Err(reject("incomplete owned extensions"));
    }
    if originals.units.iter().any(|unit| !unit.data_declarations.is_empty()) {
        return Err(reject(
            "executable Copy lane does not yet prove original nominal member obligations",
        ));
    }
    check_originals(program, originals, sources)?;
    let mut budget = Budget::default();
    for (function_index, function) in program.functions.iter().enumerate() {
        let domain = if function.key[0] == 0x40 {
            keys::Domain::FunctionInstance
        } else {
            keys::Domain::SourceRoot
        };
        let key = keys::decode(&function.key, domain)?;
        let (module, index) = key.declaration().ok_or(Failure::InternalFailure)?;
        let original = &originals.units[module as usize].functions[index as usize];
        let mut arguments = reserve(key.arguments().len())?;
        arguments.extend(key.arguments());
        let (blocks, extensions) =
            build(program, originals, sources, module, original, arguments, false, &mut budget)?;
        if function.blocks != blocks || claim.extensions[function_index] != extensions {
            return Err(reject(
                "claimed body differs from complete source operation/order/binding replay",
            ));
        }
    }
    crate::generic_v1::source_body::demand(program)
}

pub(super) fn check_originals(
    program: &raw::Program,
    originals: &Originals<'_>,
    sources: &zryna_source::SourceMap,
) -> Result<(), Failure> {
    // Every original is checked before checking instances, including unused templates.
    for unit in originals.units {
        for original in &unit.functions {
            let arity = original.type_parameters.as_ref().map_or(0, |list| list.parameters.len());
            let mut parameters = reserve(arity)?;
            for slot in 0..arity {
                // Each original body has its own isolated slot context. These one-byte opaque
                // tags cannot enter a closed key; they do not inflate the smallest substitution.
                let key = vec![0x30 + u8::try_from(slot).map_err(|_| Failure::InternalFailure)?];
                parameters.push(key);
            }
            let mut arguments = reserve(arity)?;
            arguments.extend(parameters.iter().map(Vec::as_slice));
            build(
                program,
                originals,
                sources,
                unit.id,
                original,
                arguments,
                true,
                &mut Budget::default(),
            )?;
        }
    }
    Ok(())
}

pub(super) fn produce(
    program: &mut raw::Program,
    originals: &Originals<'_>,
    sources: &zryna_source::SourceMap,
) -> Result<Vec<Vec<Extension>>, Failure> {
    let mut all = reserve(program.functions.len())?;
    let mut budget = Budget::default();
    for function in &program.functions {
        let domain = if function.key[0] == 0x40 {
            keys::Domain::FunctionInstance
        } else {
            keys::Domain::SourceRoot
        };
        let key = keys::decode(&function.key, domain)?;
        let (module, index) = key.declaration().ok_or(Failure::InternalFailure)?;
        let original = &originals.units[module as usize].functions[index as usize];
        let mut arguments = reserve(key.arguments().len())?;
        arguments.extend(key.arguments());
        all.push(build(
            program,
            originals,
            sources,
            module,
            original,
            arguments,
            false,
            &mut budget,
        )?);
    }
    let mut extensions = reserve(all.len())?;
    for (function, (blocks, ext)) in program.functions.iter_mut().zip(all) {
        function.blocks = blocks;
        extensions.push(ext);
    }
    Ok(extensions)
}

#[allow(clippy::too_many_arguments)] // Exact source inputs and separate aggregate allocation credit.
fn build<'a, 'b>(
    program: &'a raw::Program,
    originals: &'a Originals<'b>,
    sources: &'a zryna_source::SourceMap,
    module: u32,
    original: &'b RawFunctionSyntax,
    arguments: Vec<&'a [u8]>,
    symbolic: bool,
    budget: &mut Budget,
) -> Result<(Vec<raw::Block>, Vec<Extension>), Failure> {
    let resolver =
        Resolver { originals, module, parameters: original.type_parameters.as_ref(), arguments };
    let result = resolver.resolve_symbolic(original.result_type)?;
    let mut builder = Builder {
        program,
        sources,
        resolver,
        original,
        symbolic,
        blocks: reserve(1)?,
        block: 0,
        next: 0,
        locals: reserve(original.parameters.len())?,
        scope_start: 0,
        result,
        extensions: Vec::new(),
        affine: Vec::new(),
        alive: Vec::new(),
        loans: BTreeMap::new(),
        loan_parents: BTreeMap::new(),
        budget: *budget,
    };
    builder.new_block(original.body.span)?;
    for parameter in &original.parameters {
        let ty = builder.resolve(parameter.type_syntax)?;
        if matches!(ty, Closed::Unit) {
            return Err(reject("Copy executable parameters cannot carry unit or loans"));
        }
        let value = builder.value(ty)?;
        let definition = builder.definition(&value)?;
        builder.blocks[0].parameters.push(definition);
        builder.bind(&parameter.name.text, value)?;
    }
    builder.source_block(original.body.root_block, 0)?;
    normalize::run(&mut builder.blocks, &mut builder.extensions)?;
    *budget = builder.budget;
    Ok((builder.blocks, builder.extensions))
}

impl<'b> Builder<'_, 'b> {
    fn locate(&self, failure: Failure, at: zryna_source::UntrustedSpan) -> Failure {
        let Failure::Diagnostics(mut errors) = failure else {
            return failure;
        };
        for error in &mut errors {
            if error.code == "ZRYNA-M7007"
                && error.primary_span().is_none()
                && let Ok(span) = self.sources.verify_span(at)
            {
                *error = zryna_diagnostics::Diagnostic::error_at(
                    &error.code,
                    span,
                    &error.message,
                    &error.guidance,
                );
            }
        }
        Failure::Diagnostics(errors)
    }
    fn resolve(&self, occurrence: u32) -> Result<Closed, Failure> {
        if self.symbolic {
            self.resolver.resolve_symbolic(occurrence)
        } else {
            self.resolver.resolve(occurrence)
        }
    }

    fn bind(&mut self, name: &'b str, value: Value) -> Result<(), Failure> {
        if self.locals[self.scope_start..].iter().any(|prior| prior.name.eq_ignore_ascii_case(name))
        {
            return Err(reject("source value bindings collide under portable folding"));
        }
        self.locals.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
        self.locals.push(Binding { name, value, mutable: false });
        Ok(())
    }

    fn value(&mut self, ty: Closed) -> Result<Value, Failure> {
        owners::admitted(&ty)?;
        if self.next as usize >= crate::data_ownership_v1::MAX_VALUES_PER_FUNCTION {
            return Err(crate::generic_v1::budget("source replay exceeds inherited value ceiling"));
        }
        let id = self.next;
        self.budget.value()?;
        self.next = self.next.checked_add(1).ok_or(Failure::InternalFailure)?;
        let affine = owners::affine(&ty)?;
        self.affine.push(affine);
        self.alive.push(true);
        if let Closed::Borrow(_, exclusive) = &ty {
            self.loans.insert(id, (id, *exclusive));
        }
        Ok(Value { id, ty })
    }

    fn definition(&self, value: &Value) -> Result<raw::Definition, Failure> {
        Ok(raw::Definition {
            id: value.id,
            ty: if self.symbolic {
                raw::Type::Unit
            } else {
                type_id(self.program, value.ty.clone())?
            },
        })
    }

    fn new_block(&mut self, span: zryna_source::UntrustedSpan) -> Result<usize, Failure> {
        if self.blocks.len() >= crate::data_ownership_v1::MAX_BLOCKS_PER_FUNCTION {
            return Err(crate::generic_v1::budget("source replay exceeds inherited block ceiling"));
        }
        self.budget.block()?;
        self.blocks.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
        let index = self.blocks.len();
        self.blocks.push(raw::Block {
            id: u32::try_from(index).map_err(|_| Failure::InternalFailure)?,
            parameters: Vec::new(),
            instructions: Vec::new(),
            span,
            terminator: raw::Terminator::Return(u32::MAX),
        });
        Ok(index)
    }

    fn emit(
        &mut self,
        ty: Closed,
        span: zryna_source::UntrustedSpan,
        operation: raw::Operation,
    ) -> Result<Value, Failure> {
        owners::core(self, &operation)?;
        self.budget.operation(&operation)?;
        let value = self.value(ty)?;
        let result = self.definition(&value)?;
        let instructions = &mut self.blocks[self.block].instructions;
        instructions.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
        instructions.push(raw::Instruction { result, span, operation });
        Ok(value)
    }
}
