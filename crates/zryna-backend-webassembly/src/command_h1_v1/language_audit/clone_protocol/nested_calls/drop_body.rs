use super::{Diagnostic, Shape, Token as T, TypeCategory, VerifiedLayouts, VerifiedType, invalid};
use super::{address, child, constant, drop_child, load, record};

pub(super) fn derive(
    ty: VerifiedType<'_>,
    layouts: &VerifiedLayouts,
    shape: &Shape,
) -> Result<Vec<T>, Diagnostic> {
    let mut out = Vec::new();
    let category = ty.category();
    let observed = match category {
        TypeCategory::Bool | TypeCategory::I32 => None,
        TypeCategory::String => Some(1),
        TypeCategory::Struct | TypeCategory::FixedArray | TypeCategory::Vec => Some(2),
        TypeCategory::Enum => Some(3),
        TypeCategory::Shared => Some(4),
        TypeCategory::Weak => Some(5),
    };
    if let Some(kind) = observed {
        record(&mut out, kind, shape);
    }
    match category {
        TypeCategory::Struct => {
            for field in ty.fields().iter().rev() {
                drop_child(&mut out, child(layouts, field.ty())?, field.offset(), shape)?;
            }
        }
        TypeCategory::Enum => enumeration(&mut out, ty, layouts, shape)?,
        TypeCategory::FixedArray | TypeCategory::Vec => sequence(&mut out, ty, layouts, shape)?,
        TypeCategory::Shared => shared(&mut out, ty, layouts, shape)?,
        TypeCategory::Weak => decrement(&mut out, shape),
        TypeCategory::Bool | TypeCategory::I32 | TypeCategory::String => {}
    }
    out.push(T::End);
    Ok(out)
}

fn enumeration(
    out: &mut Vec<T>,
    ty: VerifiedType<'_>,
    layouts: &VerifiedLayouts,
    shape: &Shape,
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
        drop_child(out, child(layouts, id)?, offset, shape)?;
        out.push(T::End);
    }
    Ok(())
}

fn sequence(
    out: &mut Vec<T>,
    ty: VerifiedType<'_>,
    layouts: &VerifiedLayouts,
    shape: &Shape,
) -> Result<(), Diagnostic> {
    let item = child(layouts, ty.referenced_type().ok_or_else(invalid)?)?;
    if item.drop_kind() == 0 {
        return Ok(());
    }
    let vector = ty.category() == TypeCategory::Vec;
    let index = if vector { 1 } else { 3 };
    let stride = if vector { item.size().max(1) } else { ty.array_stride().ok_or_else(invalid)? };
    if vector {
        address(out, 0, 4)?;
        out.extend([T::Load, T::SetLocal(index)]);
    } else {
        out.extend([constant(ty.array_length().ok_or_else(invalid)?)?, T::SetLocal(index)]);
    }
    out.extend([
        T::Block,
        T::Loop,
        T::Local(index),
        T::Zero,
        T::BranchIf(1),
        T::Local(index),
        T::Constant(1),
        T::Subtract,
        T::Tee(index),
    ]);
    if vector {
        out.extend([T::Local(0), T::Load, T::Local(index), constant(stride)?, T::Multiply, T::Add]);
    } else {
        out.extend([constant(stride)?, T::Multiply, T::Local(0), T::Add]);
    }
    load(out, item);
    out.extend([T::Call(6 + shape.type_count + item.id().index()), T::Branch(0), T::End, T::End]);
    Ok(())
}

fn shared(
    out: &mut Vec<T>,
    ty: VerifiedType<'_>,
    layouts: &VerifiedLayouts,
    shape: &Shape,
) -> Result<(), Diagnostic> {
    out.extend([
        T::Local(0),
        T::Load,
        T::Tee(1),
        T::Zero,
        T::If,
        T::Trap,
        T::End,
        T::Local(0),
        T::Local(1),
        T::Constant(1),
        T::Subtract,
        T::Tee(1),
        T::Store,
        T::Local(1),
        T::Zero,
        T::If,
    ]);
    let item = child(layouts, ty.referenced_type().ok_or_else(invalid)?)?;
    let align = item.alignment();
    let offset = 8_u64
        .checked_add(align.checked_sub(1).ok_or_else(invalid)?)
        .map(|size| size / align * align)
        .ok_or_else(invalid)?;
    drop_child(out, item, offset, shape)?;
    record(out, 6, shape);
    decrement(out, shape);
    out.push(T::End);
    Ok(())
}

fn decrement(out: &mut Vec<T>, shape: &Shape) {
    out.extend([
        T::Local(0),
        T::Constant(4),
        T::Add,
        T::Load,
        T::Tee(1),
        T::Zero,
        T::If,
        T::Trap,
        T::End,
        T::Local(0),
        T::Constant(4),
        T::Add,
        T::Local(1),
        T::Constant(1),
        T::Subtract,
        T::Store,
        T::Local(1),
        T::Constant(1),
        T::Equal,
        T::If,
    ]);
    record(out, 7, shape);
    out.push(T::End);
}
