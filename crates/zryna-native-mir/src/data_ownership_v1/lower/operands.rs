use super::{B, VerifiedCallArgument, lowering_error, raw};

type LoweredOperands = (Vec<u32>, Vec<u32>, Vec<u32>, Option<(u32, u32)>, Vec<raw::CallArgument>);

pub(super) fn lower_operands(
    instruction: B<'_>,
) -> Result<LoweredOperands, Vec<zryna_diagnostics::Diagnostic>> {
    let mut values = Vec::new();
    let mut places = Vec::new();
    let mut borrows = Vec::new();
    let mut callee = None;
    let mut call_arguments = Vec::new();
    match instruction {
        B::EnvironmentLookup(_) => return Err(lowering_error()),
        B::BoolLiteral(_) | B::I32Literal(_) | B::String(_) => {}
        B::Binary(left, right) => values.extend([left.index(), right.index()]),
        B::Unary(value) => values.push(value.index()),
        B::DirectCall { callee: target, arguments } => {
            callee = Some((target.module(), target.declaration()));
            for argument in arguments {
                match argument {
                    VerifiedCallArgument::Value(value) => {
                        values.push(value.index());
                        call_arguments.push(raw::CallArgument::Value(value.index()));
                    }
                    VerifiedCallArgument::Borrow(borrow) => {
                        borrows.push(borrow.index());
                        call_arguments.push(raw::CallArgument::Borrow(borrow.index()));
                    }
                }
            }
        }
        B::Construct { operands, .. } | B::VecConstruct(operands) => {
            values.extend(
                operands.into_iter().map(zryna_ir::data_ownership_v1::ValueIdentity::index),
            );
        }
        B::Place(place) => places.push(place.index()),
        B::PlaceValue { place, value } => {
            places.push(place.index());
            values.push(value.index());
        }
        B::IndexedPlace { place, index } => {
            places.push(place.index());
            values.push(index.index());
        }
        B::StringConcat { left, right } => places.extend([left.index(), right.index()]),
        B::VecPush { vector, value } => {
            places.push(vector.index());
            values.push(value.index());
        }
        B::BeginBorrow(definition) => {
            places.push(definition.place().index());
            borrows.push(definition.id().index());
        }
        B::IndexedBorrow { definition, index } => {
            places.push(definition.place().index());
            values.push(index.index());
            borrows.push(definition.id().index());
        }
        B::ProjectIndexedBorrow { parent, borrow, index } => {
            values.push(index.index());
            borrows.extend([parent.index(), borrow.index()]);
        }
        B::BindIndexedBorrow { parent, borrow } => {
            borrows.extend([parent.index(), borrow.index()]);
        }
        B::BorrowValue { borrow, value } => {
            borrows.push(borrow.index());
            values.push(value.index());
        }
        B::BorrowUse(borrow) => borrows.push(borrow.index()),
    }
    Ok((values, places, borrows, callee, call_arguments))
}

#[cfg(test)]
mod tests {
    #[test]
    fn command_environment_operand_cannot_enter_ordinary_native_lowering() {
        let errors = super::lower_operands(super::B::EnvironmentLookup("MODE"))
            .expect_err("command operand requires distinct command backend authority");
        assert!(errors.iter().any(|error| error.code() == "ZRYNA-N3101"));
    }
}
