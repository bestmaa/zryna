//! Source expression typing, preserving local and exact call provenance.

use super::{BodyError, Frame, TypedExpression, Value, ValueType};
use zryna_syntax::native_c_source_v0::{MAX_EXPRESSION_DEPTH, raw as syntax};

impl Frame<'_> {
    pub(super) fn evaluate(&mut self, index: usize, depth: usize) -> Result<Value, BodyError> {
        let expression = self
            .function
            .expressions
            .get(index)
            .ok_or_else(|| self.source_error(self.function.range, "source-expression-index"))?
            .clone();
        let range = expression.range;
        if depth > MAX_EXPRESSION_DEPTH {
            return Err(self.error(range, "ZRYNA-C4107", "body-expression-depth"));
        }
        if self.typed[index].is_some() {
            return Err(self.source_error(range, "repeated-source-expression"));
        }
        let value = match &expression.kind {
            syntax::ExpressionKind::I32(_) => Value::plain(ValueType::I32),
            syntax::ExpressionKind::Bool(_) => Value::plain(ValueType::Bool),
            syntax::ExpressionKind::Key(_) => Value::plain(ValueType::Key),
            syntax::ExpressionKind::Local(name) => self.local(name, range)?,
            syntax::ExpressionKind::Add(left, right) => {
                let left = self.evaluate(*left, depth + 1)?;
                let right = self.evaluate(*right, depth + 1)?;
                if left.ty != ValueType::I32 || right.ty != ValueType::I32 {
                    return Err(self.type_error(range, "addition-type"));
                }
                for operand in [&left, &right] {
                    if operand.status.is_some_and(|call| !self.calls[call].proved_zero) {
                        return Err(self.resource_error(range, "unchecked-status-arithmetic"));
                    }
                }
                // Source addition requires wrapping i32 lowering, never an overflow trap.
                Value::plain(ValueType::I32)
            }
            syntax::ExpressionKind::Intrinsic(primitive, arguments) => {
                if self.function.exported {
                    return Err(self.type_error(range, "foreign-export-body"));
                }
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    if *argument >= index {
                        return Err(self.source_error(range, "source-expression-order"));
                    }
                    values.push(self.evaluate(*argument, depth + 1)?);
                }
                self.intrinsic(index, *primitive, arguments, &values)?
            }
        };
        self.typed[index] = Some(TypedExpression {
            range,
            ty: value.ty,
            token: value.token,
            status_call: value.status,
            source_kind: expression.kind,
        });
        Ok(value)
    }

    pub(super) fn key(&self, index: usize) -> Result<&str, BodyError> {
        match &self.function.expressions[index].kind {
            syntax::ExpressionKind::Key(key) => Ok(key),
            _ => Err(self.source_error(self.function.expressions[index].range, "literal-body-key")),
        }
    }
}
