//! Producer from the original closed semantic authority to independently verified layouts.

use zryna_layout::StorageTarget;
use zryna_layout::generic_v1::{Failure, VerifiedLayouts, raw};
use zryna_syntax::v5::RawDataDeclarationKind;

use super::{InstanceContext, TypeShape, model::Node};

/// Produces a complete closed graph and obtains a separately verified successor layout seal.
///
/// # Errors
/// Rejects unstored source members, fallible storage or any independent layout failure. Existing
/// aggregate-v1 and executable IR authorities remain unavailable through this entrypoint.
pub fn verify_layouts(
    instances: &InstanceContext<'_, '_, '_>,
    target: StorageTarget,
) -> Result<VerifiedLayouts, Failure> {
    let graph = produce(instances)?;
    let layouts = zryna_layout::generic_v1::verify(
        &graph,
        instances.bodies.declarations().sources(),
        target,
    )?;
    if layouts.types().len() != instances.type_keys().len()
        || layouts.types().zip(instances.type_keys()).any(|(ty, key)| ty.key() != key)
    {
        return Err(Failure::InternalFailure);
    }
    Ok(layouts)
}

fn produce(instances: &InstanceContext<'_, '_, '_>) -> Result<raw::Graph, Failure> {
    let context = instances.bodies.declarations();
    let mut modules = reserve(context.modules().len())?;
    let mut declarations = Vec::new();
    for module in context.modules() {
        modules.push(raw::Module {
            id: raw::ModuleId(module.identity().index()),
            source_file: module.identity().source_file(),
            data_declarations: u32::try_from(module.data_declarations().len())
                .map_err(|_| Failure::InternalFailure)?,
        });
        for declaration in module.data_declarations() {
            let owner = declaration.identity();
            let source = &context.syntax().files()[owner.module().index() as usize]
                .data_declarations[owner.source_index() as usize];
            let (kind, members) = match &source.kind {
                RawDataDeclarationKind::Struct { fields, .. } => {
                    (raw::NominalKind::Struct, fields.len())
                }
                RawDataDeclarationKind::Enum { variants, .. } => {
                    (raw::NominalKind::Enum, variants.len())
                }
            };
            declarations.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
            declarations.push(raw::Declaration {
                module: raw::ModuleId(owner.module().index()),
                index: owner.source_index(),
                kind,
                parameters: u32::try_from(declaration.type_parameters().count())
                    .map_err(|_| Failure::InternalFailure)?,
                members: u32::try_from(members).map_err(|_| Failure::InternalFailure)?,
                span: declaration.span(),
            });
        }
    }
    let mut types = reserve(instances.types.len())?;
    let mut roots = reserve(instances.types.len())?;
    for (id, node) in instances.types.iter().enumerate() {
        let id = raw::NodeId(u32::try_from(id).map_err(|_| Failure::InternalFailure)?);
        let (span, kind) = shape(instances, node)?;
        types.push(raw::TypeNode { id, span, kind });
        roots.push(id);
    }
    Ok(raw::Graph { modules, declarations, types, program_roots: roots })
}

fn shape(
    instances: &InstanceContext<'_, '_, '_>,
    node: &Node,
) -> Result<(Option<zryna_source::Span>, raw::TypeKind), Failure> {
    use zryna_layout::raw::TypeKind as Base;
    let child =
        |index: usize| node.arguments[index].ok_or(Failure::InternalFailure).and_then(node_id);
    let base = match node.shape {
        TypeShape::Bool => Some(Base::Bool),
        TypeShape::I32 => Some(Base::I32),
        TypeShape::String => Some(Base::String),
        TypeShape::Vec => Some(Base::Vec { element: child(0)? }),
        TypeShape::Shared => Some(Base::Shared { payload: child(0)? }),
        TypeShape::Weak => Some(Base::Weak { payload: child(0)? }),
        TypeShape::FixedArray(length) => {
            Some(Base::FixedArray { element: child(0)?, length: u64::from(length) })
        }
        TypeShape::Option => return Ok((None, raw::TypeKind::Option { argument: child(0)? })),
        TypeShape::Result => {
            return Ok((None, raw::TypeKind::Result { okay: child(0)?, error: child(1)? }));
        }
        TypeShape::Nominal(_) => None,
        TypeShape::Unit
        | TypeShape::Borrow
        | TypeShape::BorrowMut
        | TypeShape::Function(_)
        | TypeShape::Parameter(_) => return Err(Failure::InternalFailure),
    };
    if let Some(base) = base {
        return Ok((None, raw::TypeKind::Base(base)));
    }
    nominal(instances, node)
}

fn nominal(
    instances: &InstanceContext<'_, '_, '_>,
    node: &Node,
) -> Result<(Option<zryna_source::Span>, raw::TypeKind), Failure> {
    use zryna_layout::raw::TypeKind as Base;
    let TypeShape::Nominal(owner) = node.shape else {
        return Err(Failure::InternalFailure);
    };
    let context = instances.bodies.declarations();
    let declaration = context.declaration(owner).ok_or(Failure::InternalFailure)?;
    let source = &context.syntax().files()[owner.module().index() as usize].data_declarations
        [owner.source_index() as usize];
    let module = raw::ModuleId(owner.module().index());
    let index = owner.source_index();
    let mut arguments = reserve(2)?;
    for argument in node.arguments.into_iter().flatten() {
        arguments.push(node_id(argument)?);
    }
    let kind = match &source.kind {
        RawDataDeclarationKind::Struct { fields, .. } => {
            if fields.len() != node.members.len() {
                return Err(unstored(declaration.span()));
            }
            let mut fields = reserve(fields.len())?;
            for (ordinal, member) in node.members.iter().copied().enumerate() {
                fields.push(raw::Field {
                    ordinal: u32::try_from(ordinal).map_err(|_| Failure::InternalFailure)?,
                    ty: node_id(member)?,
                });
            }
            if arguments.is_empty() {
                raw::TypeKind::Base(Base::Struct { module, declaration: index, fields })
            } else {
                raw::TypeKind::Struct { module, declaration: index, arguments, fields }
            }
        }
        RawDataDeclarationKind::Enum { variants, .. } => {
            if variants.iter().filter(|variant| variant.payload_type.is_some()).count()
                != node.members.len()
            {
                return Err(unstored(declaration.span()));
            }
            let mut output = reserve(variants.len())?;
            let mut members = node.members.iter().copied();
            for (ordinal, variant) in variants.iter().enumerate() {
                let payload = if variant.payload_type.is_some() {
                    Some(node_id(members.next().ok_or(Failure::InternalFailure)?)?)
                } else {
                    None
                };
                output.push(raw::Variant {
                    ordinal: u32::try_from(ordinal).map_err(|_| Failure::InternalFailure)?,
                    payload,
                });
            }
            if arguments.is_empty() {
                raw::TypeKind::Base(Base::Enum { module, declaration: index, variants: output })
            } else {
                raw::TypeKind::Enum { module, declaration: index, arguments, variants: output }
            }
        }
    };
    Ok((Some(declaration.span()), kind))
}

fn unstored(span: zryna_source::Span) -> Failure {
    Failure::Diagnostics(vec![zryna_diagnostics::Diagnostic::error_at(
        "ZRYNA-L3004",
        span,
        "closed original declaration contains a member without a stored layout",
        "remove borrow, unit or function authorities from stored declarations",
    )])
}

fn node_id(id: usize) -> Result<raw::NodeId, Failure> {
    u32::try_from(id).map(raw::NodeId).map_err(|_| Failure::InternalFailure)
}

fn reserve<T>(count: usize) -> Result<Vec<T>, Failure> {
    let mut output = Vec::new();
    output.try_reserve_exact(count).map_err(|_| Failure::AllocationFailure)?;
    Ok(output)
}
