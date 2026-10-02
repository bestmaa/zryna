use super::{Diagnostic, Shape, TypeCategory, VerifiedLayouts, VerifiedType, invalid};
use super::{
    Token as T, address, child, constant, drop_child, failed, indexed, load, record, store,
};

pub(super) fn derive(
    ty: VerifiedType<'_>,
    layouts: &VerifiedLayouts,
    shape: &Shape,
    enabled: bool,
) -> Result<Vec<T>, Diagnostic> {
    let mut out = vec![T::Tee(2), T::Local(0)];
    if ty.category() == TypeCategory::Vec {
        let element = child(layouts, ty.referenced_type().ok_or_else(invalid)?)?;
        out.extend([
            T::Local(1),
            constant(element.size().max(1))?,
            T::Multiply,
            T::Constant(12),
            T::Add,
            T::Call(1),
        ]);
        out.extend([T::Local(2), T::Local(2), T::Constant(12), T::Add, T::Store]);
    } else {
        out.extend([constant(ty.size())?, T::Call(1)]);
    }
    match ty.category() {
        TypeCategory::Struct => structure(&mut out, ty, layouts, shape, enabled)?,
        TypeCategory::Enum => enumeration(&mut out, ty, layouts, shape, enabled)?,
        TypeCategory::FixedArray | TypeCategory::Vec => {
            sequence(&mut out, ty, layouts, shape, enabled)?;
        }
        _ => return Err(invalid()),
    }
    out.extend([T::Local(2), T::End]);
    Ok(out)
}

fn structure(
    out: &mut Vec<T>,
    ty: VerifiedType<'_>,
    layouts: &VerifiedLayouts,
    shape: &Shape,
    enabled: bool,
) -> Result<(), Diagnostic> {
    for (ordinal, field) in ty.fields().iter().enumerate() {
        let item = child(layouts, field.ty())?;
        if item.drop_kind() == 0 {
            continue;
        }
        address_pair(out, item, field.offset())?;
        failure_start(out, enabled, shape);
        record(out, 2, shape);
        for previous in ty.fields()[..ordinal].iter().rev() {
            drop_child(out, child(layouts, previous.ty())?, previous.offset(), shape)?;
        }
        failure_end(out);
        store(out, item)?;
    }
    Ok(())
}

fn enumeration(
    out: &mut Vec<T>,
    ty: VerifiedType<'_>,
    layouts: &VerifiedLayouts,
    shape: &Shape,
    enabled: bool,
) -> Result<(), Diagnostic> {
    let offset = ty.enum_payload_layout().ok_or_else(invalid)?.0;
    for variant in ty.variants() {
        let Some(id) = variant.payload() else { continue };
        out.extend([
            T::Local(0),
            T::Load,
            T::Constant(i32::try_from(variant.ordinal()).map_err(|_| invalid())?),
            T::Equal,
            T::If,
        ]);
        let item = child(layouts, id)?;
        if item.drop_kind() != 0 {
            address_pair(out, item, offset)?;
            failure_start(out, enabled, shape);
            if enabled {
                record(out, 3, shape);
            }
            failure_end(out);
            store(out, item)?;
        }
        out.push(T::End);
    }
    Ok(())
}

fn address_pair(out: &mut Vec<T>, ty: VerifiedType<'_>, offset: u64) -> Result<(), Diagnostic> {
    address(out, 2, offset)?;
    address(out, 0, offset)?;
    load(out, ty);
    out.push(T::Call(6 + ty.id().index()));
    Ok(())
}

fn failure_start(out: &mut Vec<T>, enabled: bool, shape: &Shape) {
    out.extend([T::Global(1), T::If]);
    failed(out, enabled, shape);
    out.extend([T::Local(2), T::SetLocal(0)]);
}

fn failure_end(out: &mut Vec<T>) {
    out.extend([T::Constant(0), T::Return, T::End]);
}

fn sequence(
    out: &mut Vec<T>,
    ty: VerifiedType<'_>,
    layouts: &VerifiedLayouts,
    shape: &Shape,
    enabled: bool,
) -> Result<(), Diagnostic> {
    let item = child(layouts, ty.referenced_type().ok_or_else(invalid)?)?;
    if item.drop_kind() == 0 {
        return Ok(());
    }
    let header = ty.category() == TypeCategory::Vec;
    let stride = if header { item.size().max(1) } else { ty.array_stride().ok_or_else(invalid)? };
    out.extend([T::Constant(0), T::SetLocal(3), T::Block, T::Loop, T::Local(3)]);
    out.push(if header { T::Local(1) } else { constant(ty.array_length().ok_or_else(invalid)?)? });
    out.extend([T::GreaterEqual, T::BranchIf(1)]);
    indexed(out, 2, 3, stride, header)?;
    indexed(out, 0, 3, stride, header)?;
    load(out, item);
    out.extend([T::Call(6 + item.id().index()), T::Global(1), T::If]);
    failed(out, enabled, shape);
    record(out, 2, shape);
    out.extend([
        T::Block,
        T::Loop,
        T::Local(3),
        T::Zero,
        T::BranchIf(1),
        T::Local(3),
        T::Constant(1),
        T::Subtract,
        T::SetLocal(3),
    ]);
    indexed(out, 2, 3, stride, header)?;
    load(out, item);
    out.extend([
        T::Call(6 + shape.type_count + item.id().index()),
        T::Branch(0),
        T::End,
        T::End,
        T::Constant(0),
        T::Return,
        T::End,
    ]);
    store(out, item)?;
    out.extend([T::Local(3), T::Constant(1), T::Add, T::SetLocal(3), T::Branch(0), T::End, T::End]);
    Ok(())
}
