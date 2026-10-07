//! Source/family enum identities, active payloads and canonical exhaustive match transfers.

use super::{Builder, Value};
use crate::generic_v1::{
    Failure, raw, reject, reserve,
    source::Target,
    source_types::{Closed, Resolver, encode_mode, type_id},
};
use zryna_syntax::{v4::RawMatchArm, v5::RawDataDeclarationKind};

pub(super) struct Description<'a> {
    pub ty: Closed,
    pub variants: Vec<(&'a str, Option<Closed>)>,
}

pub(super) fn describe<'a>(
    resolver: &Resolver<'_, 'a>,
    name: &str,
    arguments: &[Vec<u8>],
    symbolic: bool,
) -> Result<Description<'a>, Failure> {
    let mut variants = reserve(2)?;
    let (tag, lanes) = match name {
        "Option" if arguments.len() == 1 => {
            variants.extend([("none", None), ("some", Some(Closed::Stored(arguments[0].clone())))]);
            (0x14, vec![1])
        }
        "Result" if arguments.len() == 2 => {
            variants.extend([
                ("ok", Some(Closed::Stored(arguments[0].clone()))),
                ("err", Some(Closed::Stored(arguments[1].clone()))),
            ]);
            (0x15, vec![2])
        }
        "Option" | "Result" => {
            return Err(reject("compiler enum family requires exact explicit arity"));
        }
        _ => {
            let Target::Data(module, index) = resolver.originals.resolve(resolver.module, name)?
            else {
                return Err(reject("enum constructor names a function"));
            };
            let original =
                &resolver.originals.units[module as usize].data_declarations[index as usize];
            let RawDataDeclarationKind::Enum { variants: declared, .. } = &original.kind else {
                return Err(reject("enum constructor names a struct"));
            };
            let arity = original.type_parameters.as_ref().map_or(0, |list| list.parameters.len());
            if arity != arguments.len() {
                return Err(reject("nominal enum has wrong generic arity"));
            }
            let mut keys = reserve(arity)?;
            keys.extend(arguments.iter().map(Vec::as_slice));
            let substitution = Resolver {
                originals: resolver.originals,
                module,
                parameters: original.type_parameters.as_ref(),
                arguments: keys,
            };
            variants.try_reserve_exact(declared.len()).map_err(|_| Failure::AllocationFailure)?;
            for variant in declared {
                let payload = variant
                    .payload_type
                    .map(|occurrence| substitution.resolve_symbolic(occurrence))
                    .transpose()?;
                if payload.as_ref().is_some_and(|ty| !matches!(ty, Closed::Stored(_))) {
                    return Err(reject("enum payload is not a stored value"));
                }
                variants.push((&variant.name.text, payload));
            }
            if arity == 0 {
                (0x11, vec![module, index])
            } else {
                (
                    0x13,
                    vec![
                        module,
                        index,
                        u32::try_from(arity).map_err(|_| Failure::InternalFailure)?,
                    ],
                )
            }
        }
    };
    Ok(Description { ty: Closed::Stored(encode_mode(tag, &lanes, arguments, symbolic)?), variants })
}

fn arguments(key: &[u8]) -> Result<Vec<Vec<u8>>, Failure> {
    let (offset, count) = match key.first() {
        Some(0x14) => (5, 1),
        Some(0x15) => (5, 2),
        Some(0x11) => (9, 0),
        Some(0x13) => (
            13,
            u32::from_le_bytes(
                key.get(9..13)
                    .ok_or(Failure::InternalFailure)?
                    .try_into()
                    .map_err(|_| Failure::InternalFailure)?,
            ) as usize,
        ),
        _ => return Err(reject("source match requires an original or compiler-owned enum")),
    };
    let mut result = reserve(count)?;
    let mut cursor = offset;
    for _ in 0..count {
        let length = u32::from_le_bytes(
            key.get(cursor..cursor + 4)
                .ok_or(Failure::InternalFailure)?
                .try_into()
                .map_err(|_| Failure::InternalFailure)?,
        ) as usize;
        cursor += 4;
        let end = cursor.checked_add(length).ok_or(Failure::InternalFailure)?;
        let mut child = reserve(length)?;
        child.extend_from_slice(key.get(cursor..end).ok_or(Failure::InternalFailure)?);
        result.push(child);
        cursor = end;
    }
    if cursor != key.len() {
        return Err(Failure::InternalFailure);
    }
    Ok(result)
}

impl<'b> Builder<'_, 'b> {
    #[allow(
        clippy::too_many_lines,
        reason = "The original arm replay and edge-specific affine merge form one source authentication step."
    )]
    pub(super) fn match_expression(
        &mut self,
        expression: u32,
        arms: &'b [RawMatchArm],
        span: zryna_source::UntrustedSpan,
        depth: usize,
    ) -> Result<Value, Failure> {
        let scrutinee = self.expression(expression, depth + 1)?;
        let (key, mode) = match &scrutinee.ty {
            Closed::Stored(key) => (key.clone(), raw::MatchMode::Value),
            Closed::Borrow(key, false) => (key.clone(), raw::MatchMode::SharedBorrow),
            Closed::Borrow(key, true) => (key.clone(), raw::MatchMode::ExclusiveBorrow),
            Closed::Unit => return Err(reject("cannot match unit")),
        };
        let first = arms.first().ok_or_else(|| reject("empty match"))?;
        let description =
            describe(&self.resolver, &first.type_name.text, &arguments(&key)?, self.symbolic)?;
        if description.ty != Closed::Stored(key.clone()) || arms.len() != description.variants.len()
        {
            return Err(reject("inexact enum match coverage"));
        }
        let entry = self.block;
        let saved = self.locals.clone();
        let prior_scope = self.scope_start;
        if mode == raw::MatchMode::Value && self.affine[scrutinee.id as usize] {
            self.consume(scrutinee.id)?;
        }
        let saved_alive = self.alive.clone();
        let saved_loans = self.loans.clone();
        let saved_parents = self.loan_parents.clone();
        let mut successors = reserve(arms.len())?;
        let mut outputs = reserve(arms.len())?;
        for (ordinal, (name, payload)) in description.variants.iter().enumerate() {
            let mut selected = arms.iter().filter(|a| a.variant.text == *name);
            let arm = selected.next().ok_or_else(|| reject("missing enum ordinal"))?;
            if selected.next().is_some()
                || describe(&self.resolver, &arm.type_name.text, &arguments(&key)?, self.symbolic)?
                    .ty
                    != description.ty
            {
                return Err(reject("duplicate/foreign enum match arm"));
            }
            self.locals = saved.clone();
            self.scope_start = saved.len();
            self.alive.clone_from(&saved_alive);
            self.alive.resize(self.next as usize, false);
            self.loans = saved_loans.clone();
            self.loan_parents = saved_parents.clone();
            self.block = self.new_block(arm.span)?;
            let target = self.block;
            let from = self.next as usize;
            let binding = match (&arm.binding, payload) {
                (Some(name), Some(ty)) => {
                    let ty = if mode == raw::MatchMode::Value {
                        ty.clone()
                    } else {
                        let Closed::Stored(key) = ty else {
                            return Err(Failure::InternalFailure);
                        };
                        Closed::Borrow(key.clone(), mode == raw::MatchMode::ExclusiveBorrow)
                    };
                    let value = self.value(ty)?;
                    let definition = self.definition(&value)?;
                    self.blocks[target].parameters.push(definition.clone());
                    if mode != raw::MatchMode::Value {
                        let root = *self.loans.get(&scrutinee.id).ok_or_else(|| {
                            super::owners::ownership("missing retained source loan")
                        })?;
                        self.loans.insert(value.id, root);
                        self.loan_parents.insert(value.id, scrutinee.id);
                    }
                    self.bind(&name.text, value)?;
                    Some(definition)
                }
                (None, None) => None,
                _ => return Err(reject("inexact active payload binding")),
            };
            let value = self.expression(arm.value, depth + 1)?;
            if matches!(value.ty, Closed::Borrow(..)) {
                return Err(super::owners::ownership("loan cannot escape match result"));
            }
            self.close_scope(from, Some(value.id), arm.span)?;
            outputs.push((self.block, value, self.alive.clone()));
            successors.push(raw::Arm {
                ordinal: u32::try_from(ordinal).map_err(|_| Failure::InternalFailure)?,
                binding,
                edge: raw::Edge {
                    target: u32::try_from(target).map_err(|_| Failure::InternalFailure)?,
                    arguments: Vec::new(),
                },
            });
        }
        let result_type = outputs[0].1.ty.clone();
        if outputs.iter().any(|(_, v, _)| v.ty != result_type) {
            return Err(reject("different enum arm result types"));
        }
        let mut common = saved_alive.clone();
        for (id, available) in common.iter_mut().enumerate() {
            *available &= outputs.iter().all(|(_, _, s)| s.get(id).copied().unwrap_or(false));
        }
        for (block, value, state) in &outputs {
            self.block = *block;
            self.alive.clone_from(state);
            self.alive.resize(self.next as usize, false);
            self.loans = saved_loans.clone();
            self.loan_parents = saved_parents.clone();
            for id in (0..common.len()).rev() {
                if self.affine[id] && self.alive[id] && !common[id] {
                    self.ext(
                        Closed::Unit,
                        span,
                        super::Owned::Drop(
                            u32::try_from(id).map_err(|_| Failure::InternalFailure)?,
                        ),
                    )?;
                }
            }
            // The edge transfers each arm result into one new joined owner.
            if self.affine[value.id as usize] {
                self.consume(value.id)?;
            }
        }
        self.alive = common;
        self.alive.resize(self.next as usize, false);
        self.loans = saved_loans;
        self.loan_parents = saved_parents;
        self.block = self.new_block(span)?;
        let value = self.value(result_type)?;
        let definition = self.definition(&value)?;
        self.blocks[self.block].parameters.push(definition);
        for (block, out, _) in outputs {
            self.blocks[block].terminator = raw::Terminator::Jump(raw::Edge {
                target: u32::try_from(self.block).map_err(|_| Failure::InternalFailure)?,
                arguments: vec![out.id],
            });
        }
        let ty = if self.symbolic {
            0
        } else {
            let raw::Type::Stored(id) = type_id(self.program, Closed::Stored(key))? else {
                return Err(Failure::InternalFailure);
            };
            id
        };
        self.blocks[entry].span = span;
        self.blocks[entry].terminator = raw::Terminator::ClosedEnumMatch {
            ty,
            scrutinee: scrutinee.id,
            mode,
            arms: successors,
        };
        self.locals = saved;
        self.scope_start = prior_scope;
        if mode != raw::MatchMode::Value
            && matches!(
                self.original.body.expressions[expression as usize].kind,
                zryna_syntax::v5::RawExpressionKind::Borrow { .. }
                    | zryna_syntax::v5::RawExpressionKind::BorrowMut { .. }
            )
        {
            self.ext(Closed::Unit, span, super::Owned::EndLoan(scrutinee.id))?;
        }
        Ok(value)
    }
}
