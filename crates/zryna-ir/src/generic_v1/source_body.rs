//! Complete source replay for the first executable Copy-only successor lane.
//!
//! Unsupported source operations fail closed, including in unused original templates.
//! The symbolic pass retains opaque parameter keys; a closed i32 instance cannot legalize T + T.

use super::{
    Failure, keys, raw, reject, reserve,
    source::Originals,
    source_types::{Closed, Resolver, type_id},
};
use zryna_syntax::{v4::RawStatementKind, v5::RawFunctionSyntax};

mod enums;
mod expressions;
mod opaque_owners;

#[derive(Clone)]
struct Value {
    id: u32,
    ty: Closed,
}

struct Builder<'a, 'b> {
    program: &'a raw::Program,
    resolver: Resolver<'a, 'b>,
    original: &'b RawFunctionSyntax,
    symbolic: bool,
    blocks: Vec<raw::Block>,
    block: usize,
    next: u32,
    scope_start: usize,
    locals: Vec<(&'b str, Value)>,
    result: Closed,
}

pub(super) fn check(program: &raw::Program, originals: &Originals<'_>) -> Result<(), Failure> {
    if originals.units.iter().any(|unit| !unit.data_declarations.is_empty()) {
        return Err(reject(
            "executable Copy lane does not yet prove original nominal member obligations",
        ));
    }
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
            build(program, originals, unit.id, original, arguments, true)?;
        }
    }
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
        let blocks = build(program, originals, module, original, arguments, false)?;
        if function.blocks != blocks {
            return Err(reject(
                "claimed body differs from complete source operation/order/binding replay",
            ));
        }
    }
    demand(program)
}

pub(super) fn produce(
    program: &mut raw::Program,
    originals: &Originals<'_>,
) -> Result<(), Failure> {
    let mut all = reserve(program.functions.len())?;
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
        all.push(build(program, originals, module, original, arguments, false)?);
    }
    for (function, blocks) in program.functions.iter_mut().zip(all) {
        function.blocks = blocks;
    }
    check(program, originals)
}

fn build<'a, 'b>(
    program: &'a raw::Program,
    originals: &'a Originals<'b>,
    module: u32,
    original: &'b RawFunctionSyntax,
    arguments: Vec<&'a [u8]>,
    symbolic: bool,
) -> Result<Vec<raw::Block>, Failure> {
    let resolver =
        Resolver { originals, module, parameters: original.type_parameters.as_ref(), arguments };
    let result = resolver.resolve_symbolic(original.result_type)?;
    let mut builder = Builder {
        program,
        resolver,
        original,
        symbolic,
        blocks: reserve(1)?,
        block: 0,
        next: 0,
        locals: reserve(original.parameters.len())?,
        scope_start: 0,
        result,
    };
    builder.new_block(original.body.span)?;
    for parameter in &original.parameters {
        let ty = builder.resolve(parameter.type_syntax)?;
        if matches!(ty, Closed::Unit | Closed::Borrow(..)) {
            return Err(reject("Copy executable parameters cannot carry unit or loans"));
        }
        let value = builder.value(ty)?;
        let definition = builder.definition(&value)?;
        builder.blocks[0].parameters.push(definition);
        builder.bind(&parameter.name.text, value)?;
    }
    builder.source_block(original.body.root_block, 0)?;
    Ok(builder.blocks)
}

impl<'b> Builder<'_, 'b> {
    fn resolve(&self, occurrence: u32) -> Result<Closed, Failure> {
        if self.symbolic {
            self.resolver.resolve_symbolic(occurrence)
        } else {
            self.resolver.resolve(occurrence)
        }
    }

    fn bind(&mut self, name: &'b str, value: Value) -> Result<(), Failure> {
        if self.symbolic {
            opaque_owners::check_binding(self.original, name, &value.ty)?;
        }
        if self.locals[self.scope_start..].iter().any(|(prior, _)| prior.eq_ignore_ascii_case(name))
        {
            return Err(reject("source value bindings collide under portable folding"));
        }
        self.locals.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
        self.locals.push((name, value));
        Ok(())
    }

    fn value(&mut self, ty: Closed) -> Result<Value, Failure> {
        if self.next as usize >= crate::data_ownership_v1::MAX_VALUES_PER_FUNCTION {
            return Err(super::budget("source replay exceeds inherited value ceiling"));
        }
        let id = self.next;
        self.next = self.next.checked_add(1).ok_or(Failure::InternalFailure)?;
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
            return Err(super::budget("source replay exceeds inherited block ceiling"));
        }
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
        let value = self.value(ty)?;
        let result = self.definition(&value)?;
        let instructions = &mut self.blocks[self.block].instructions;
        instructions.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
        instructions.push(raw::Instruction { result, span, operation });
        Ok(value)
    }

    fn source_block(&mut self, index: u32, depth: usize) -> Result<(), Failure> {
        if depth > 128 {
            return Err(super::budget("source replay nesting exceeds 128"));
        }
        let source = &self.original.body.blocks[index as usize];
        let mut returned = false;
        for id in &source.statements {
            if returned {
                return Err(reject("Copy executable lane does not admit statements after return"));
            }
            let statement = &self.original.body.statements[*id as usize];
            match &statement.kind {
                RawStatementKind::LocalDeclaration {
                    name,
                    type_syntax,
                    initializer,
                    mutable,
                    ..
                } => {
                    if *mutable {
                        return Err(reject(
                            "mutable source state requires successor ownership/CFG replay",
                        ));
                    }
                    let value = self.expression(*initializer, depth + 1)?;
                    if value.ty != self.resolve(*type_syntax)? {
                        return Err(reject(
                            "source initializer differs from its declared symbolic type",
                        ));
                    }
                    self.bind(&name.text, value)?;
                }
                RawStatementKind::Return { value, .. } => {
                    let value = self.expression(*value, depth + 1)?;
                    if value.ty != self.result {
                        return Err(reject("source return differs from original symbolic result"));
                    }
                    self.blocks[self.block].span = statement.span;
                    self.blocks[self.block].terminator = raw::Terminator::Return(value.id);
                    returned = true;
                }
                RawStatementKind::ExpressionStatement { expression, .. } => {
                    self.expression(*expression, depth + 1)?;
                }
                _ => {
                    return Err(reject(
                        "source statement requires a successor lane beyond immutable Copy replay",
                    ));
                }
            }
        }
        if !returned {
            if self.result != Closed::Unit {
                return Err(reject("source function lacks an exact return"));
            }
            let value = self.emit(Closed::Unit, source.span, raw::Operation::Unit)?;
            self.blocks[self.block].span = source.span;
            self.blocks[self.block].terminator = raw::Terminator::Return(value.id);
        }
        Ok(())
    }
}

fn demand(program: &raw::Program) -> Result<(), Failure> {
    let mut reached = reserve(program.functions.len())?;
    reached.resize(program.functions.len(), false);
    let mut pending = reserve(program.functions.len())?;
    for (index, function) in program.functions.iter().enumerate() {
        if function.key[0] == 0x41 {
            reached[index] = true;
            pending.push(index);
        }
    }
    while let Some(index) = pending.pop() {
        for instruction in
            program.functions[index].blocks.iter().flat_map(|block| &block.instructions)
        {
            if let raw::Operation::ClosedGenericCall { instance, .. } = instruction.operation {
                let target = instance as usize;
                let seen =
                    reached.get_mut(target).ok_or_else(|| reject("demanded instance is absent"))?;
                if !*seen {
                    *seen = true;
                    pending.push(target);
                }
            }
        }
    }
    if reached.iter().any(|seen| !seen) {
        return Err(reject("generic inventory contains an undemanded instance"));
    }
    Ok(())
}
