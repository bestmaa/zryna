use super::model::{Builder, Environment, Node};
use super::{
    DeclarationIdentity, InstantiationFailure, MAX_DEPTH, MAX_KEY_BYTES, TypeShape, TypeView,
    budget, failure, keys, push,
};
use zryna_source::UntrustedSpan;

struct Frame<'v, 'c, 's> {
    view: TypeView<'v, 'c, 's>,
    children: [Option<TypeView<'v, 'c, 's>>; 2],
    count: usize,
    arguments: [Option<usize>; 2],
    next: usize,
}

impl<'v, 'c, 's> Frame<'v, 'c, 's> {
    fn new(view: TypeView<'v, 'c, 's>) -> Self {
        let mut children = [None; 2];
        let mut count = 0;
        for (index, child) in view.children().enumerate() {
            children[index] = Some(child);
            count += 1;
        }
        Self { view, children, count, arguments: [None; 2], next: 0 }
    }
}

pub(super) fn close(
    builder: &mut Builder<'_, '_, '_>,
    view: TypeView<'_, '_, '_>,
    environment: Environment,
    from: &[u8],
    generated: Option<usize>,
    at: Option<UntrustedSpan>,
) -> Result<Option<usize>, InstantiationFailure> {
    let mut stack = super::reserve(65)?;
    stack.push(Frame::new(view));
    let mut result = None;
    while let Some(frame) = stack.last_mut() {
        if frame.next < frame.count {
            let child = frame.children[frame.next].ok_or(InstantiationFailure::InternalFailure)?;
            frame.next += 1;
            push(&mut stack, Frame::new(child))?;
            continue;
        }
        let frame = stack.pop().ok_or(InstantiationFailure::InternalFailure)?;
        let at = frame.view.application_head_span().or(at);
        result = match frame.view.shape() {
            TypeShape::Parameter(parameter) => {
                if parameter.declaration() == environment.owner {
                    environment.arguments[parameter.index() as usize]
                } else {
                    None
                }
            }
            TypeShape::Unit | TypeShape::Borrow | TypeShape::BorrowMut | TypeShape::Function(_) => {
                None
            }
            _shape if frame.arguments.iter().take(frame.count).any(Option::is_none) => None,
            shape => Some(intern(
                builder,
                shape,
                frame.arguments,
                frame.children.map(|child| child.and_then(TypeView::original_span)),
                at,
            )?),
        };
        if let Some(id) = result {
            dependency(builder, from, id, at)?;
            // A parameter substitution copies a finite supplied subtree. Only an application
            // written by the declaration itself participates in generated-expansion paths.
            if !matches!(frame.view.shape(), TypeShape::Parameter(_))
                && let Some(origin) = generated
                && builder.generic(builder.types[id].shape)
            {
                generated_edge(builder, origin, id, at)?;
            }
        }
        if let Some(parent) = stack.last_mut() {
            parent.arguments[parent.next - 1] = result;
        }
    }
    Ok(result)
}

fn intern(
    builder: &mut Builder<'_, '_, '_>,
    shape: TypeShape,
    arguments: [Option<usize>; 2],
    argument_spans: [Option<UntrustedSpan>; 2],
    at: Option<UntrustedSpan>,
) -> Result<usize, InstantiationFailure> {
    let children =
        arguments.iter().flatten().map(|id| builder.types[*id].key.as_slice()).collect::<Vec<_>>();
    let child_depth = arguments.iter().flatten().map(|id| builder.types[*id].depth).max();
    let depth = child_depth.map_or(0, |depth| depth + 1);
    if depth > MAX_DEPTH {
        return Err(budget(
            builder.bodies,
            "type application depth",
            MAX_DEPTH as usize,
            depth as usize,
            at,
        ));
    }
    let (tag, lanes) = match shape {
        TypeShape::Bool => (0, vec![]),
        TypeShape::I32 => (1, vec![]),
        TypeShape::String => (2, vec![]),
        TypeShape::Option => (0x14, vec![1]),
        TypeShape::Result => (0x15, vec![2]),
        TypeShape::Vec => (0x21, vec![]),
        TypeShape::Shared => (0x22, vec![]),
        TypeShape::Weak => (0x23, vec![]),
        TypeShape::FixedArray(length) => (0x20, vec![length]),
        TypeShape::Nominal(owner) => {
            if owner.kind() == super::DeclarationKind::Function {
                return Err(InstantiationFailure::InternalFailure);
            }
            let data = &builder.bodies.declarations().syntax().files()
                [owner.module().index() as usize]
                .data_declarations[owner.source_index() as usize];
            let enumeration =
                matches!(data.kind, zryna_syntax::v5::RawDataDeclarationKind::Enum { .. });
            let mut lanes = vec![owner.module().index(), owner.source_index()];
            if !children.is_empty() {
                lanes.push(
                    u32::try_from(children.len())
                        .map_err(|_| InstantiationFailure::InternalFailure)?,
                );
            }
            (
                if children.is_empty() {
                    if enumeration { 0x11 } else { 0x10 }
                } else if enumeration {
                    0x13
                } else {
                    0x12
                },
                lanes,
            )
        }
        _ => return Err(InstantiationFailure::InternalFailure),
    };
    let (key, key_bytes) = keys::encode(tag, &lanes, &children)?;
    if key.is_empty() {
        return Err(budget(builder.bodies, "instance key bytes", MAX_KEY_BYTES, key_bytes, at));
    }
    if builder.generic(shape) {
        for (argument, span) in arguments.into_iter().zip(argument_spans) {
            if let Some(argument) = argument {
                builder.argument_use(argument, span.or(at))?;
            }
        }
    }
    let value = match shape {
        TypeShape::Nominal(owner) => builder.bounds.value(owner)?,
        _ => true,
    } && arguments.into_iter().flatten().all(|id| builder.types[id].value);
    builder.intern(
        Node { key, shape, arguments, depth, processed: false, members: Vec::new(), value },
        at,
    )
}

pub(super) fn nominal(
    builder: &mut Builder<'_, '_, '_>,
    owner: DeclarationIdentity,
    arguments: [Option<usize>; 2],
    at: Option<UntrustedSpan>,
) -> Result<usize, InstantiationFailure> {
    intern(builder, TypeShape::Nominal(owner), arguments, [at; 2], at)
}

pub(super) fn dependency(
    builder: &mut Builder<'_, '_, '_>,
    from: &[u8],
    id: usize,
    at: Option<UntrustedSpan>,
) -> Result<(), InstantiationFailure> {
    if from.is_empty() {
        return Ok(());
    }
    let mut pending = super::reserve(65)?;
    pending.push(id);
    while let Some(id) = pending.pop() {
        if builder.generic(builder.types[id].shape) {
            let key = super::copy_bytes(&builder.types[id].key)?;
            builder.edge(from, &key, at)?;
        }
        for child in builder.types[id].arguments.into_iter().flatten() {
            push(&mut pending, child)?;
        }
    }
    Ok(())
}

fn generated_edge(
    builder: &mut Builder<'_, '_, '_>,
    from: usize,
    to: usize,
    at: Option<UntrustedSpan>,
) -> Result<(), InstantiationFailure> {
    let Err(position) = builder.generated.binary_search(&(from, to)) else {
        return Ok(());
    };
    if let TypeShape::Nominal(target) = builder.types[to].shape
        && builder.generic(builder.types[to].shape)
    {
        let mut pending = super::reserve(builder.types.len())?;
        let mut visited = super::reserve(builder.types.len())?;
        visited.resize(builder.types.len(), false);
        pending.push(from);
        while let Some(id) = pending.pop() {
            if visited[id] {
                continue;
            }
            visited[id] = true;
            if builder.types[id].shape == TypeShape::Nominal(target) && id != to {
                return Err(failure(builder.bodies,"ZRYNA-M7003",at,"declaration-generated application repeats a generic declaration with different closed arguments".into()));
            }
            for &parent in &builder.generated_reverse[id] {
                push(&mut pending, parent)?;
            }
        }
    }
    builder.generated.try_reserve(1).map_err(|_| InstantiationFailure::AllocationFailure)?;
    builder.generated_reverse[to]
        .try_reserve(1)
        .map_err(|_| InstantiationFailure::AllocationFailure)?;
    builder.generated.insert(position, (from, to));
    builder.generated_reverse[to].push(from);
    Ok(())
}

pub(super) fn source(
    builder: &mut Builder<'_, '_, '_>,
    environment: Environment,
    occurrence: u32,
    from: &[u8],
    generated: Option<usize>,
) -> Result<Option<usize>, InstantiationFailure> {
    let Some(view) = builder.bodies.source_type(environment.owner, occurrence) else {
        return Ok(None);
    };
    let node = &builder.bodies.declarations().syntax().files()
        [environment.owner.module().index() as usize]
        .type_syntax[occurrence as usize];
    close(builder, view, environment, from, generated, Some(node.span))
}
