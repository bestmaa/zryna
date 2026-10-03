//! Nominal value bounds over original declarations, without specialization or field expansion.

use super::{
    BodyTypeContext, DeclarationIdentity, DeclarationKind, InstantiationFailure, TypeShape, push,
    reserve,
};

pub(super) struct Bounds {
    owners: Vec<DeclarationIdentity>,
    values: Vec<bool>,
}

impl Bounds {
    pub(super) fn new(bodies: &BodyTypeContext<'_, '_>) -> Result<Self, InstantiationFailure> {
        let declarations = bodies.declarations();
        let count = super::checked_count(
            declarations.syntax().files().iter().map(|file| file.data_declarations.len()),
        )?;
        let mut owners = reserve(count)?;
        owners.extend(
            declarations
                .modules()
                .flat_map(super::super::ModuleView::data_declarations)
                .map(super::super::DeclarationView::identity),
        );
        owners.sort_unstable();
        let mut values = reserve(count)?;
        values.resize(count, true);
        let mut reverse = reserve(count)?;
        reverse.resize_with(count, Vec::new);
        for module in declarations.modules() {
            for (owner, occurrence) in bodies.source_owners(module.identity().index()) {
                if owner.kind() == DeclarationKind::Function {
                    continue;
                }
                let index = owners
                    .binary_search(&owner)
                    .map_err(|_| InstantiationFailure::InternalFailure)?;
                let view = bodies
                    .source_type(
                        owner,
                        u32::try_from(occurrence)
                            .map_err(|_| InstantiationFailure::InternalFailure)?,
                    )
                    .ok_or(InstantiationFailure::InternalFailure)?;
                match view.shape() {
                    TypeShape::Unit
                    | TypeShape::Borrow
                    | TypeShape::BorrowMut
                    | TypeShape::Function(_) => values[index] = false,
                    TypeShape::Nominal(target) => {
                        let target = owners
                            .binary_search(&target)
                            .map_err(|_| InstantiationFailure::InternalFailure)?;
                        push(&mut reverse[target], index)?;
                    }
                    // Every opaque parameter is constrained to ZrynaValue. Original source
                    // occurrences include all argument/element children, so no normalized
                    // substituted tree or recursive nominal unfolding is necessary here.
                    _ => {}
                }
            }
        }
        let mut pending = reserve(count)?;
        pending.extend(
            values.iter().enumerate().filter(|(_, value)| !**value).map(|(index, _)| index),
        );
        while let Some(index) = pending.pop() {
            for &parent in &reverse[index] {
                if values[parent] {
                    values[parent] = false;
                    push(&mut pending, parent)?;
                }
            }
        }
        Ok(Self { owners, values })
    }

    pub(super) fn value(&self, owner: DeclarationIdentity) -> Result<bool, InstantiationFailure> {
        let index =
            self.owners.binary_search(&owner).map_err(|_| InstantiationFailure::InternalFailure)?;
        Ok(self.values[index])
    }
}
