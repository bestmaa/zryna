//! Exact call provenance, nonzero-domain dispatch and status-zero dominance.

use super::{BodyError, FailureRoute, FlowStep, Frame, Value, ValueType};
use zryna_syntax::{native_c_source_v0::raw as syntax, native_c_v0::raw::StatusKind};

impl Frame<'_> {
    pub(super) fn guard(
        &mut self,
        name: &str,
        expression: usize,
        range: syntax::Range,
    ) -> Result<(), BodyError> {
        let value = self.local(name, range)?;
        let call =
            value.status.ok_or_else(|| self.resource_error(range, "guard-status-provenance"))?;
        if value.ty != ValueType::I32 || !self.calls[call].status_mode {
            return Err(self.type_error(range, "guard-status-type"));
        }
        self.guard_call = Some(call);
        let terminal = self.evaluate(expression, 1)?;
        self.guard_call = None;
        if terminal.ty != ValueType::Terminal || terminal.status != Some(call) {
            return Err(self.resource_error(range, "guard-terminal-provenance"));
        }
        let operation = &self.declarations.declarations.operations[self.calls[call].operation];
        // Empty recoverable sets are valid: unknown nonzero is already a call-boundary failure.
        let recoverable = operation
            .statuses
            .iter()
            .filter(|status| status.kind == StatusKind::Recoverable)
            .map(|status| status.code)
            .collect();
        self.steps.push(FlowStep::StatusGuard {
            expression,
            call,
            recoverable,
            recoverable_route: FailureRoute::DeclaredForeignError,
            cleanup: self.cleanup(Some(call)),
        });
        self.calls[call].proved_zero = true;
        Ok(())
    }

    pub(super) fn foreign_error(
        &self,
        expression: usize,
        key: usize,
        status: &Value,
    ) -> Result<Value, BodyError> {
        let range = self.function.expressions[expression].range;
        let call = status
            .status
            .ok_or_else(|| self.resource_error(range, "foreign-error-status-provenance"))?;
        if status.ty != ValueType::I32 || self.guard_call != Some(call) {
            return Err(self.resource_error(range, "foreign-error-outside-exact-guard"));
        }
        let operation = &self.declarations.declarations.operations[self.calls[call].operation];
        if self.key(key)? != operation.key || !self.calls[call].status_mode {
            return Err(self.resource_error(range, "foreign-error-operation-provenance"));
        }
        Ok(Value { status: Some(call), ..Value::plain(ValueType::Terminal) })
    }
}
