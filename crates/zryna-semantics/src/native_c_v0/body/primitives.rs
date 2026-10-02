//! All fourteen closed source primitives, without aliases or inferred C resource identities.

use super::{BodyError, Frame, Value, ValueType};
use zryna_syntax::native_c_v0::raw::{Access, Category, Mode, Primitive};

impl Frame<'_> {
    pub(super) fn intrinsic(
        &mut self,
        expression: usize,
        primitive: Primitive,
        arguments: &[usize],
        values: &[Value],
    ) -> Result<Value, BodyError> {
        match primitive {
            Primitive::RawCall => {
                self.raw_call(expression, arguments[0], &arguments[1..], &values[1..])
            }
            Primitive::BorrowBytes => {
                self.prepare_loan(expression, arguments[0], &values[0], false)
            }
            Primitive::BorrowUtf8 => self.prepare_loan(expression, arguments[0], &values[0], true),
            Primitive::ByteLength => {
                let loan = self.token(&values[0], ValueType::Bytes, expression)?;
                Ok(Value { length_of: Some(loan), ..Value::plain(ValueType::I32) })
            }
            Primitive::OutI32 => Ok(self.output_slot(expression, ValueType::I32Out, None)),
            Primitive::OutHandle => {
                let kind = self.key(arguments[0])?.to_owned();
                if !self.declarations.declarations.libraries.iter().any(|library| {
                    library.allocators.iter().any(|allocator| {
                        allocator.kind == kind && allocator.category == Category::Handle
                    })
                }) {
                    return Err(self.resource_error(
                        self.function.expressions[expression].range,
                        "unknown-handle-output-kind",
                    ));
                }
                Ok(self.output_slot(expression, ValueType::HandleOut, Some(kind)))
            }
            Primitive::OutBytes => Ok(self.output_slot(expression, ValueType::BytesOut, None)),
            Primitive::OutCount => Ok(self.output_slot(expression, ValueType::CountOut, None)),
            Primitive::ReadI32 => self.read_i32(expression, &values[0]),
            Primitive::TakeHandle => self.take_handle(expression, &values[0]),
            Primitive::TakeBytes => self.take_bytes(expression, &values[0], &values[1]),
            Primitive::CopyBytes => self.copy(expression, &values[0]),
            Primitive::Release => {
                let operation = self.operation_index(self.key(arguments[0])?, expression)?;
                let declaration = &self.declarations.declarations.operations[operation];
                if declaration.mode != Mode::Void
                    || declaration.resources.len() != 1
                    || declaration.resources[0].access != Access::Consume
                {
                    return Err(self.resource_error(
                        self.function.expressions[expression].range,
                        "release-operation-shape",
                    ));
                }
                self.call(expression, operation, &arguments[1..], &values[1..], true)
            }
            Primitive::ForeignError => self.foreign_error(expression, arguments[0], &values[1]),
        }
    }
}
