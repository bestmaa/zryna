//! Exact source-to-ABI argument typing and physical output alias rejection.

use super::{BodyError, BoundaryCheck, Frame, Value, ValueType};
use std::collections::BTreeSet;
use zryna_syntax::native_c_v0::raw::{AbiType, Mode, Operation};

pub(super) struct Arguments {
    pub outputs: Vec<usize>,
    pub checks: Vec<BoundaryCheck>,
}

fn expected_type(abi: AbiType) -> Option<ValueType> {
    Some(match abi {
        AbiType::CI32 | AbiType::CInt | AbiType::Count => ValueType::I32,
        AbiType::Bool32 => ValueType::Bool,
        AbiType::BytesIn => ValueType::Bytes,
        AbiType::BytesOwnedOut => ValueType::BytesOut,
        AbiType::CountOut => ValueType::CountOut,
        AbiType::I32Out => ValueType::I32Out,
        AbiType::HandleIn => ValueType::Handle,
        AbiType::HandleOut => ValueType::HandleOut,
        AbiType::BytesRelease => ValueType::OwnedBytes,
        AbiType::Unit => return None,
    })
}

impl Frame<'_> {
    pub(super) fn check_arguments(
        &self,
        expression: usize,
        operation: &Operation,
        arguments: &[usize],
        values: &[Value],
    ) -> Result<Arguments, BodyError> {
        let range = self.function.expressions[expression].range;
        if arguments.len() != operation.parameters.len() || values.len() != arguments.len() {
            return Err(self.type_error(range, "raw-call-arity"));
        }
        let mut outputs = Vec::new();
        let mut checks = Vec::new();
        for ((parameter, value), argument) in operation.parameters.iter().zip(values).zip(arguments)
        {
            let expected = expected_type(parameter.abi)
                .ok_or_else(|| self.type_error(range, "unit-abi-parameter"))?;
            if value.ty != expected {
                return Err(self.type_error(
                    self.function.expressions[*argument].range,
                    "raw-call-argument-type",
                ));
            }
            if value.status.is_some_and(|call| !self.calls[call].proved_zero) {
                return Err(self.resource_error(range, "unchecked-status-as-call-argument"));
            }
            if !expected.scalar() {
                self.token(value, expected, *argument)?;
            }
            if matches!(
                expected,
                ValueType::I32Out
                    | ValueType::HandleOut
                    | ValueType::BytesOut
                    | ValueType::CountOut
            ) {
                let token = self.token(value, expected, *argument)?;
                if expected != ValueType::I32Out && self.tokens[token].output.is_some() {
                    return Err(self.resource_error(range, "resource-output-slot-reused"));
                }
                outputs.push(token);
            }
            match parameter.abi {
                AbiType::Count => {
                    checks.push(BoundaryCheck::CountConversion { expression: *argument });
                }
                AbiType::Bool32 => {
                    checks.push(BoundaryCheck::BooleanCarrier { expression: *argument });
                }
                _ => {}
            }
        }
        if outputs.iter().copied().collect::<BTreeSet<_>>().len() != outputs.len() {
            return Err(self.resource_error(range, "aliased-output-slots"));
        }
        if !outputs.is_empty() && operation.mode != Mode::Status {
            return Err(self.resource_error(range, "output-without-status-channel"));
        }
        Ok(Arguments { outputs, checks })
    }
}
