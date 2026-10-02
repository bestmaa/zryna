//! Independent definition, operand, lexical binding and private-move replay.

use crate::{IrError, raw, require};
use std::collections::BTreeMap;
use zryna_semantics::native_c_v0::body::{PrivateOrigin, TypedFunction, ValueType};
use zryna_syntax::{native_c_source_v0::raw as syntax, native_c_v0::raw::Primitive};

struct Binding<'a> {
    name: &'a str,
    ty: ValueType,
    origin: Option<PrivateOrigin>,
    available: bool,
}

pub(super) fn check(claim: &raw::Function, original: &TypedFunction) -> Result<(), IrError> {
    let mut bindings = Vec::new();
    let mut names = BTreeMap::new();
    for (index, parameter) in original.parameters().iter().enumerate() {
        let ty = ValueType::from(parameter.ty);
        let origin = matches!(ty, ValueType::String | ValueType::VecI32)
            .then_some(PrivateOrigin::Parameter(index));
        bindings.push(Binding { name: &parameter.name, ty, origin, available: true });
        names.insert(parameter.name.as_str(), index);
    }
    let mut next = 0;
    for statement in original.statements() {
        let root = match &statement.kind {
            syntax::StatementKind::Const(_, root)
            | syntax::StatementKind::Return(root)
            | syntax::StatementKind::Guard(_, root)
            | syntax::StatementKind::Expression(root) => *root,
        };
        require(next <= root && root < claim.values.len(), "ZRYNA-C4106", "ir-statement-arena")?;
        while next <= root {
            let value = &claim.values[next];
            let source = &original.expressions()[next];
            require(value.ty == source.value_type(), "ZRYNA-C4104", "ir-value-type")?;
            require(
                value.token == source.token_id() && value.status_call == source.status_call(),
                "ZRYNA-C4105",
                "ir-value-provenance",
            )?;
            let origin = match (&value.kind, source.source_kind()) {
                (raw::ValueKind::I32(a), syntax::ExpressionKind::I32(b)) if a == b => None,
                (raw::ValueKind::Bool(a), syntax::ExpressionKind::Bool(b)) if a == b => None,
                (raw::ValueKind::Key(a), syntax::ExpressionKind::Key(b)) if a == b => None,
                (raw::ValueKind::Local(id), syntax::ExpressionKind::Local(name)) => {
                    let binding = bindings
                        .get(*id)
                        .ok_or_else(|| IrError::new("ZRYNA-C4106", "ir-local-identity"))?;
                    require(
                        names.get(name.as_str()) == Some(id)
                            && binding.name == name
                            && binding.ty == value.ty,
                        "ZRYNA-C4106",
                        "ir-local-identity",
                    )?;
                    require(binding.available, "ZRYNA-C4105", "ir-use-after-private-move")?;
                    binding.origin
                }
                (raw::ValueKind::WrappingAdd(a, b), syntax::ExpressionKind::Add(x, y))
                    if a == x && b == y =>
                {
                    require(
                        *a < next
                            && *b < next
                            && claim.values[*a].ty == ValueType::I32
                            && claim.values[*b].ty == ValueType::I32
                            && value.ty == ValueType::I32,
                        "ZRYNA-C4104",
                        "ir-wrapping-add-operands",
                    )?;
                    None
                }
                (
                    raw::ValueKind::Primitive(p, args),
                    syntax::ExpressionKind::Intrinsic(q, original_args),
                ) if p == q && args == original_args => {
                    require(
                        args.iter().all(|id| *id < next),
                        "ZRYNA-C4106",
                        "ir-definition-before-use",
                    )?;
                    if *p == Primitive::CopyBytes { Some(PrivateOrigin::Copy(next)) } else { None }
                }
                _ => return Err(IrError::new("ZRYNA-C4106", "ir-original-value-operation")),
            };
            require(value.origin == origin, "ZRYNA-C4105", "ir-private-move-origin")?;
            next += 1;
        }
        if let syntax::StatementKind::Const(binding, _) = &statement.kind {
            let value = &claim.values[root];
            require(value.ty == ValueType::from(binding.ty), "ZRYNA-C4104", "ir-binding-type")?;
            if value.origin.is_some()
                && let raw::ValueKind::Local(id) = &value.kind
            {
                bindings[*id].available = false;
            }
            require(
                !names.contains_key(binding.name.as_str()),
                "ZRYNA-C4106",
                "ir-duplicate-binding",
            )?;
            names.insert(binding.name.as_str(), bindings.len());
            bindings.push(Binding {
                name: &binding.name,
                ty: value.ty,
                origin: value.origin,
                available: true,
            });
        }
    }
    require(next == claim.values.len(), "ZRYNA-C4106", "ir-unaccounted-definition")
}
