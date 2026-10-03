use super::model::{Builder, Environment};
use super::types;
use super::{DeclarationKind, InstantiationFailure, push, reserve};
use zryna_syntax::v5::{RawDataDeclarationKind, RawExpressionKind};

pub(super) fn roots(builder: &mut Builder<'_, '_, '_>) -> Result<(), InstantiationFailure> {
    let declarations = builder.bodies.declarations();
    for module in declarations.modules() {
        let raw_types =
            &declarations.syntax().files()[module.identity().index() as usize].type_syntax;
        let mut occurrences = reserve(raw_types.len())?;
        occurrences.extend(builder.bodies.source_owners(module.identity().index()));
        occurrences.sort_unstable_by_key(|(owner, id)| {
            (*owner, raw_types[*id].span.start, raw_types[*id].span.end, *id)
        });
        for declaration in module.data_declarations().chain(module.functions()) {
            let owner = declaration.identity();
            if declaration.type_parameters().next().is_none() {
                if owner.kind() == DeclarationKind::Function {
                    builder.function(owner, [None; 2], None)?;
                } else {
                    types::nominal(builder, owner, [None; 2], None)?;
                }
            }
            // Fully closed original type occurrences are roots even in otherwise unused
            // templates. An unbound parameter cannot become a concrete inventory member.
            let environment = Environment { owner, arguments: [None; 2] };
            let start = occurrences.partition_point(|(candidate, _)| *candidate < owner);
            let end = occurrences.partition_point(|(candidate, _)| *candidate <= owner);
            for (_, id) in &occurrences[start..end] {
                let occurrence =
                    u32::try_from(*id).map_err(|_| InstantiationFailure::InternalFailure)?;
                let ty = types::source(builder, environment, occurrence, &[], None)?;
                if builder.bodies.source_argument(owner, occurrence)
                    && let Some(ty) = ty
                {
                    builder.argument_use(ty, Some(raw_types[*id].span))?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn pending(builder: &mut Builder<'_, '_, '_>) -> Result<(), InstantiationFailure> {
    loop {
        let ty = builder.type_order.iter().copied().find(|id| !builder.types[*id].processed);
        let function =
            builder.function_order.iter().copied().find(|id| !builder.functions[*id].processed);
        match (ty, function) {
            (Some(ty), Some(function))
                if builder.functions[function].key < builder.types[ty].key =>
            {
                process_function(builder, function)?;
            }
            (Some(ty), _) => process_type(builder, ty)?,
            (None, Some(function)) => process_function(builder, function)?,
            (None, None) => break,
        }
    }
    Ok(())
}

fn process_type(builder: &mut Builder<'_, '_, '_>, id: usize) -> Result<(), InstantiationFailure> {
    builder.types[id].processed = true;
    if matches!(builder.types[id].shape, super::TypeShape::Option | super::TypeShape::Result) {
        let key = super::copy_bytes(&builder.types[id].key)?;
        for member in builder.types[id].arguments.into_iter().flatten() {
            types::dependency(builder, &key, member, None)?;
        }
        return Ok(());
    }
    let super::TypeShape::Nominal(owner) = builder.types[id].shape else {
        return Ok(());
    };
    let environment = Environment { owner, arguments: builder.types[id].arguments };
    let key = super::copy_bytes(&builder.types[id].key)?;
    let data = &builder.bodies.declarations().syntax().files()[owner.module().index() as usize]
        .data_declarations[owner.source_index() as usize];
    match &data.kind {
        RawDataDeclarationKind::Struct { fields, .. } => {
            for field in fields {
                member(builder, id, environment, field.type_syntax, &key)?;
            }
        }
        RawDataDeclarationKind::Enum { variants, .. } => {
            for variant in variants {
                if let Some(payload) = variant.payload_type {
                    member(builder, id, environment, payload, &key)?;
                }
            }
        }
    }
    Ok(())
}

fn process_function(
    builder: &mut Builder<'_, '_, '_>,
    id: usize,
) -> Result<(), InstantiationFailure> {
    builder.functions[id].processed = true;
    let owner = builder.functions[id].owner;
    let environment = Environment { owner, arguments: builder.functions[id].arguments };
    let key = super::copy_bytes(&builder.functions[id].key)?;
    let function = &builder.bodies.declarations().syntax().files()[owner.module().index() as usize]
        .functions[owner.source_index() as usize];
    for parameter in &function.parameters {
        types::source(builder, environment, parameter.type_syntax, &key, None)?;
    }
    types::source(builder, environment, function.result_type, &key, None)?;
    for statement in &function.body.statements {
        if let zryna_syntax::v4::RawStatementKind::LocalDeclaration { type_syntax, .. } =
            statement.kind
        {
            types::source(builder, environment, type_syntax, &key, None)?;
        }
    }
    for (index, expression) in function.body.expressions.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| InstantiationFailure::InternalFailure)?;
        if let Some(view) = builder.bodies.expression_type(owner, index) {
            types::close(builder, view, environment, &key, None, Some(expression.span))?;
        }
        if let RawExpressionKind::Call { callee, type_arguments, .. } = &expression.kind {
            let target =
                builder.target(owner, &callee.text).ok_or(InstantiationFailure::InternalFailure)?;
            let mut arguments = [None; 2];
            if let Some(list) = type_arguments {
                for (index, occurrence) in list.arguments.iter().enumerate() {
                    arguments[index] =
                        types::source(builder, environment, *occurrence, &key, None)?;
                    if let Some(argument) = arguments[index] {
                        let span = builder.bodies.declarations().syntax().files()
                            [owner.module().index() as usize]
                            .type_syntax[*occurrence as usize]
                            .span;
                        builder.argument_use(argument, Some(span))?;
                    }
                    if arguments[index].is_none() {
                        return Err(InstantiationFailure::InternalFailure);
                    }
                }
            }
            let target_id = builder.function(target, arguments, Some(callee.span))?;
            if builder.functions[target_id].generic {
                let target_key = super::copy_bytes(&builder.functions[target_id].key)?;
                builder.edge(&key, &target_key, Some(callee.span))?;
            }
        }
    }
    Ok(())
}

fn member(
    builder: &mut Builder<'_, '_, '_>,
    id: usize,
    environment: Environment,
    occurrence: u32,
    key: &[u8],
) -> Result<(), InstantiationFailure> {
    match types::source(builder, environment, occurrence, key, Some(id))? {
        Some(member) => push(&mut builder.types[id].members, member)?,
        None => builder.types[id].value = false,
    }
    Ok(())
}

pub(super) fn value_arguments(
    builder: &mut Builder<'_, '_, '_>,
) -> Result<(), InstantiationFailure> {
    let count = builder.types.len();
    let mut reverse = reserve(count)?;
    reverse.resize_with(count, Vec::new);
    let mut pending = reserve(count)?;
    for (id, node) in builder.types.iter().enumerate() {
        if !node.value {
            pending.push(id);
        }
        for child in node.arguments.into_iter().flatten().chain(node.members.iter().copied()) {
            push(&mut reverse[child], id)?;
        }
    }
    while let Some(id) = pending.pop() {
        for &parent in &reverse[id] {
            if builder.types[parent].value {
                builder.types[parent].value = false;
                push(&mut pending, parent)?;
            }
        }
    }
    for &(id, at) in &builder.argument_uses {
        if !builder.types[id].value {
            builder.errors.at(
                builder.bodies,
                "ZRYNA-M7001",
                at,
                &builder.types[id].key,
                &[],
                "closed argument contains a non-storable nominal member".into(),
            )?;
        }
    }
    Ok(())
}
