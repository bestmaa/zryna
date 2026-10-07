//! Exact original operand order and symbolic source call substitution.

use super::{Builder, Value};
use crate::generic_v1::{
    Failure, raw, reject, reserve,
    source::Target,
    source_types::{Closed, Resolver},
};
use zryna_syntax::v5::RawExpressionKind as Expr;

impl Builder<'_, '_> {
    pub(super) fn expression(&mut self, index: u32, depth: usize) -> Result<Value, Failure> {
        if depth > 128 {
            return Err(crate::generic_v1::budget("source expression depth exceeds 128"));
        }
        let source = &self.original.body.expressions[index as usize];
        match &source.kind {
            Expr::Reference { name } => self
                .locals
                .iter()
                .rev()
                .find(|(local, _)| *local == name.text)
                .map(|(_, value)| value.clone())
                .ok_or_else(|| reject("source reference has no exact local binding")),
            Expr::BoolLiteral { value } => {
                self.emit(Closed::Stored(vec![0]), source.span, raw::Operation::BoolLiteral(*value))
            }
            Expr::I32Literal { spelling } => {
                let value = spelling
                    .parse::<i32>()
                    .map_err(|_| reject("source i32 literal is outside the exact scalar range"))?;
                self.emit(Closed::Stored(vec![1]), source.span, raw::Operation::I32Literal(value))
            }
            Expr::Addition { lhs, rhs, .. } => {
                let left = self.expression(*lhs, depth + 1)?;
                let right = self.expression(*rhs, depth + 1)?;
                if left.ty != Closed::Stored(vec![1]) || right.ty != Closed::Stored(vec![1]) {
                    return Err(reject(
                        "source addition requires original i32 operands; opaque T cannot specialize it",
                    ));
                }
                self.emit(
                    left.ty,
                    source.span,
                    raw::Operation::I32Add { left: left.id, right: right.id },
                )
            }
            Expr::Call { callee, type_arguments, arguments, .. } => {
                self.call(callee, type_arguments.as_ref(), arguments, source.span, depth)
            }
            Expr::EnumConstruction { type_name, variant, type_arguments, payload, .. } => self
                .construct_enum(
                    type_name,
                    variant,
                    type_arguments.as_ref(),
                    *payload,
                    source.span,
                    depth,
                ),
            Expr::Match { scrutinee, arms, .. } => {
                self.match_expression(*scrutinee, arms, source.span, depth + 1)
            }
            _ => Err(reject(
                "source expression requires a successor lane beyond immutable Copy replay",
            )),
        }
    }
    fn call(
        &mut self,
        callee: &zryna_syntax::v4::RawIdentifierSyntax,
        type_arguments: Option<&zryna_syntax::v5::RawTypeArgumentList>,
        arguments: &[u32],
        span: zryna_source::UntrustedSpan,
        depth: usize,
    ) -> Result<Value, Failure> {
        if self.locals.iter().any(|(local, _)| *local == callee.text) {
            return Err(reject(
                "source call names a lexical value rather than its shadowed original function",
            ));
        }
        let Target::Function(module, function) =
            self.resolver.originals.resolve(self.resolver.module, &callee.text)?
        else {
            return Err(reject("source call target is not an original function"));
        };
        let original = &self.resolver.originals.units[module as usize].functions[function as usize];
        let arity = original.type_parameters.as_ref().map_or(0, |list| list.parameters.len());
        if arity != type_arguments.map_or(0, |list| list.arguments.len())
            || (arity == 0 && type_arguments.is_some())
        {
            return Err(reject("source call requires the exact explicit generic arity"));
        }
        let mut types = reserve(arity)?;
        for occurrence in type_arguments.map_or(&[][..], |list| &list.arguments) {
            let Closed::Stored(key) = self.resolve(*occurrence)? else {
                return Err(reject("generic call argument is not a stored value type"));
            };
            types.push(key);
        }
        let mut keys = reserve(arity)?;
        keys.extend(types.iter().map(Vec::as_slice));
        let resolver = Resolver {
            originals: self.resolver.originals,
            module,
            parameters: original.type_parameters.as_ref(),
            arguments: keys,
        };
        if arguments.len() != original.parameters.len() {
            return Err(reject("source call has wrong value arity"));
        }
        let mut operands = reserve(arguments.len())?;
        for (argument, parameter) in arguments.iter().zip(&original.parameters) {
            let value = self.expression(*argument, depth + 1)?;
            if value.ty != resolver.resolve_symbolic(parameter.type_syntax)? {
                return Err(reject("source call operand is not exact original substitution"));
            }
            operands.push(value.id);
        }
        let result = resolver.resolve_symbolic(original.result_type)?;
        let operation = if arity == 0 {
            raw::Operation::SourceCall { module, function, arguments: operands }
        } else {
            let instance = if self.symbolic {
                0
            } else {
                let key = function_key(module, function, &types)?;
                let index = self
                    .program
                    .functions
                    .binary_search_by(|candidate| candidate.key.cmp(&key))
                    .map_err(|_| reject("complete demanded generic instance is absent"))?;
                u32::try_from(index).map_err(|_| Failure::InternalFailure)?
            };
            raw::Operation::ClosedGenericCall { instance, arguments: operands }
        };
        self.emit(result, span, operation)
    }

    fn construct_enum(
        &mut self,
        type_name: &zryna_syntax::v4::RawIdentifierSyntax,
        variant: &zryna_syntax::v4::RawIdentifierSyntax,
        type_arguments: Option<&zryna_syntax::v5::RawTypeArgumentList>,
        payload: Option<u32>,
        span: zryna_source::UntrustedSpan,
        depth: usize,
    ) -> Result<Value, Failure> {
        let mut arguments = reserve(type_arguments.map_or(0, |list| list.arguments.len()))?;
        for occurrence in type_arguments.map_or(&[][..], |list| &list.arguments) {
            let Closed::Stored(key) = self.resolve(*occurrence)? else {
                return Err(reject("enum argument is not a stored type"));
            };
            arguments.push(key);
        }
        let description =
            super::enums::describe(&self.resolver, &type_name.text, &arguments, self.symbolic)?;
        let ordinal = description
            .variants
            .iter()
            .position(|(name, _)| *name == variant.text)
            .ok_or_else(|| reject("source constructor names an unknown enum variant"))?;
        let value = payload.map(|payload| self.expression(payload, depth + 1)).transpose()?;
        if value.as_ref().map(|value| &value.ty) != description.variants[ordinal].1.as_ref() {
            return Err(reject(
                "source enum constructor payload differs from exact original active variant",
            ));
        }
        let ty = if self.symbolic {
            0
        } else {
            let raw::Type::Stored(id) =
                crate::generic_v1::source_types::type_id(self.program, description.ty.clone())?
            else {
                return Err(Failure::InternalFailure);
            };
            id
        };
        self.emit(
            description.ty,
            span,
            raw::Operation::ClosedEnumConstruct {
                ty,
                ordinal: u32::try_from(ordinal).map_err(|_| Failure::InternalFailure)?,
                payload: value.map(|value| value.id),
            },
        )
    }
}

pub(super) fn function_key(
    module: u32,
    function: u32,
    arguments: &[Vec<u8>],
) -> Result<Vec<u8>, Failure> {
    let count = arguments
        .iter()
        .try_fold(13usize, |length, key| length.checked_add(4)?.checked_add(key.len()))
        .ok_or(Failure::InternalFailure)?;
    if count > 4096 {
        return Err(crate::generic_v1::budget("source call key exceeds 4096 bytes"));
    }
    let mut key = reserve(count)?;
    key.push(0x40);
    for lane in
        [module, function, u32::try_from(arguments.len()).map_err(|_| Failure::InternalFailure)?]
    {
        key.extend_from_slice(&lane.to_le_bytes());
    }
    for argument in arguments {
        key.extend_from_slice(
            &u32::try_from(argument.len()).map_err(|_| Failure::InternalFailure)?.to_le_bytes(),
        );
        key.extend_from_slice(argument);
    }
    Ok(key)
}
