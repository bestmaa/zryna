//! Exact import signatures, output/group identity and pre-effect acquisition requirements.

use super::{
    BodyError, BoundaryCheck, Call, CallEntry, FailureRoute, FlowStep, Frame,
    MAX_LIVE_FOREIGN_OBLIGATIONS, TrapRequirement, Value, ValueType, state::Output,
};
use zryna_syntax::native_c_v0::raw::{AbiType, Category, Direction, Mode, Safety, StatusKind};

impl Frame<'_> {
    pub(super) fn operation_index(&self, key: &str, expression: usize) -> Result<usize, BodyError> {
        self.declarations
            .declarations
            .operations
            .iter()
            .position(|operation| operation.direction == Direction::Import && operation.key == key)
            .ok_or_else(|| {
                self.source_error(self.function.expressions[expression].range, "body-import-key")
            })
    }

    pub(super) fn raw_call(
        &mut self,
        expression: usize,
        key: usize,
        arguments: &[usize],
        values: &[Value],
    ) -> Result<Value, BodyError> {
        let operation = self.operation_index(self.key(key)?, expression)?;
        self.call(expression, operation, arguments, values, false)
    }

    pub(super) fn call(
        &mut self,
        expression: usize,
        operation_index: usize,
        arguments: &[usize],
        values: &[Value],
        safe_release: bool,
    ) -> Result<Value, BodyError> {
        let operation = self.declarations.declarations.operations[operation_index].clone();
        let range = self.function.expressions[expression].range;
        let call = self.calls.len();
        let super::arguments::Arguments { outputs, mut checks } =
            self.check_arguments(expression, &operation, arguments, values)?;
        let super::acquisitions::Resources { created, consumed } = self.plan_resources(
            expression,
            operation_index,
            &operation,
            arguments,
            values,
            &mut checks,
        )?;
        let before_call = self.cleanup(Some(call));
        self.steps.push(FlowStep::Reserve {
            expression,
            call,
            maximum_new_owners: created.len(),
            live_limit: MAX_LIVE_FOREIGN_OBLIGATIONS,
            trap: TrapRequirement::ForeignResourceLimit,
            cleanup: before_call.clone(),
        });
        for output in &outputs {
            if self.tokens[*output].ty == ValueType::I32Out {
                self.tokens[*output].output = Some(Output { call, group: None, owner: None });
            }
        }
        let recoverable = operation
            .statuses
            .iter()
            .filter(|status| status.kind == StatusKind::Recoverable)
            .map(|status| status.code)
            .collect();
        if operation.result == AbiType::Bool32 {
            checks.push(BoundaryCheck::BooleanResult);
        }
        self.calls.push(Call {
            operation: operation_index,
            proved_zero: false,
            status_mode: operation.mode == Mode::Status,
        });
        let unknown_status_unresolved_owners = created.clone();
        let entry = if safe_release
            && consumed.first().is_some_and(|owner| self.owners[*owner].category == Category::Bytes)
        {
            CallEntry::NonEmptyOwner(consumed[0])
        } else {
            CallEntry::Always
        };
        self.steps.push(FlowStep::Call {
            expression,
            call,
            operation: operation_index,
            safety: if safe_release { Safety::Safe } else { Safety::UnsafeRaw },
            entry,
            carriers: operation.parameters.iter().map(|parameter| parameter.abi).collect(),
            outputs,
            created_owners: created,
            recoverable,
            boundary_checks: checks,
            unknown_status_unresolved_owners,
            unknown_status_route: FailureRoute::HostAbiFailure,
            process_fault_route: FailureRoute::ProcessFailureNoCleanupGuarantee,
            cleanup: before_call,
        });
        for owner in consumed {
            self.consume(expression, call, operation_index, owner);
        }
        let ty = match operation.result {
            AbiType::CI32 | AbiType::CInt => ValueType::I32,
            AbiType::Bool32 => ValueType::Bool,
            AbiType::Unit => ValueType::Unit,
            _ => return Err(self.type_error(range, "raw-call-result-carrier")),
        };
        Ok(Value {
            status: if operation.mode == Mode::Status { Some(call) } else { None },
            ..Value::plain(ty)
        })
    }
}
