//! Complete closed stored demand, including generic arguments and transitive layout children.

use super::{Failure, VerifiedLayouts, raw, reject, reserve};
use crate::generic_v1::keys;

pub(super) fn check(program: &raw::Program, layouts: &VerifiedLayouts) -> Result<(), Failure> {
    let mut types = reserve(layouts.types().len())?;
    types.extend(layouts.types());
    let mut reached = reserve(types.len())?;
    reached.resize(types.len(), false);
    let mut pending = reserve(types.len())?;
    let mut mark = |index: usize| -> Result<(), Failure> {
        let seen =
            reached.get_mut(index).ok_or_else(|| reject("demanded stored type is absent"))?;
        if !*seen {
            *seen = true;
            pending.push(index);
        }
        Ok(())
    };
    for index in 0..3 {
        mark(index)?;
    } // Frozen primitive prefix retained by semantic discovery.
    for function in &program.functions {
        let domain = if function.key[0] == 0x40 {
            keys::Domain::FunctionInstance
        } else {
            keys::Domain::SourceRoot
        };
        for argument in keys::decode(&function.key, domain)?.arguments() {
            mark(
                program
                    .type_keys
                    .binary_search_by(|key| key.as_slice().cmp(argument))
                    .map_err(|_| reject("generic type argument is absent from stored demand"))?,
            )?;
        }
        let stored =
            function.parameters.iter().copied().chain(std::iter::once(function.result)).chain(
                function.blocks.iter().flat_map(|block| {
                    block
                        .parameters
                        .iter()
                        .map(|value| value.ty)
                        .chain(block.instructions.iter().map(|instruction| instruction.result.ty))
                }),
            );
        for ty in stored {
            match ty {
                raw::Type::Stored(id) => mark(id as usize)?,
                raw::Type::Borrow { referent, .. } => mark(referent as usize)?,
                raw::Type::Unit => {}
            }
        }
    }
    while let Some(index) = pending.pop() {
        let view = types[index];
        let children = view
            .arguments()
            .chain(view.referenced_type())
            .chain(view.fields().map(|(_, ty, _)| ty))
            .chain(view.variants().filter_map(|(_, ty)| ty));
        for child in children {
            let index = child.index() as usize;
            if !reached[index] {
                reached[index] = true;
                pending.push(index);
            }
        }
    }
    if reached.iter().any(|seen| !seen) {
        return Err(reject("closed type universe contains an undemanded stored record"));
    }
    Ok(())
}
