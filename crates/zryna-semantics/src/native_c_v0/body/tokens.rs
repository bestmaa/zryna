//! Nominal live owners, consumption and retained private preparation requirements.

use super::{BodyError, FailureRoute, FlowStep, Frame, TrapRequirement, Value, ValueType};
use zryna_syntax::native_c_v0::raw::{Category, Resource};

impl Frame<'_> {
    pub(super) fn owner(
        &self,
        value: &Value,
        category: Category,
        expression: usize,
    ) -> Result<usize, BodyError> {
        let ty = match category {
            Category::Handle => ValueType::Handle,
            Category::Bytes => ValueType::OwnedBytes,
        };
        let token = self.token(value, ty, expression)?;
        let range = self.function.expressions[expression].range;
        let owner =
            self.tokens[token].owner.ok_or_else(|| self.resource_error(range, "owner-origin"))?;
        if !self.owners[owner].live || !self.owners[owner].validated {
            return Err(self.resource_error(range, "owner-not-live-validated"));
        }
        Ok(owner)
    }

    pub(super) fn nominal(
        &self,
        owner: usize,
        operation: usize,
        resource: &Resource,
        expression: usize,
    ) -> Result<(), BodyError> {
        let origin = &self.owners[owner];
        let operation = &self.declarations.declarations.operations[operation];
        if origin.library != operation.library
            || origin.kind != resource.kind
            || origin.allocator != resource.allocator
            || origin.release != resource.release
        {
            return Err(self.resource_error(
                self.function.expressions[expression].range,
                "owner-nominal-identity",
            ));
        }
        Ok(())
    }

    pub(super) fn consume(
        &mut self,
        expression: usize,
        call: usize,
        operation: usize,
        owner: usize,
    ) {
        // The failure snapshot retains this owner: only a confirmed C return consumes it.
        self.steps.push(FlowStep::ConfirmRelease {
            expression,
            call,
            owner,
            operation,
            unresolved_on_fault: self.cleanup(None),
            fault_route: FailureRoute::ReleaseFailureOverridesUnresolved,
        });
        self.owners[owner].live = false;
        if let Some(token) = self.owners[owner].token {
            self.tokens[token].live = false;
        }
    }

    pub(super) fn prepare_loan(
        &mut self,
        expression: usize,
        source_expression: usize,
        value: &Value,
        utf8: bool,
    ) -> Result<Value, BodyError> {
        let expected = if utf8 { ValueType::String } else { ValueType::VecI32 };
        if value.ty != expected {
            return Err(
                self.type_error(self.function.expressions[expression].range, "loan-source-type")
            );
        }
        let loan = self.new_token(ValueType::Bytes, None, None);
        let token = self.tokens.len() - 1;
        let mut traps = vec![
            TrapRequirement::ForeignLength,
            TrapRequirement::PreservePrivatePreparationIdentity,
        ];
        if !utf8 {
            traps.push(TrapRequirement::ForeignByteRange);
        }
        self.steps.push(FlowStep::PrepareLoan {
            expression,
            token,
            source_expression,
            source_binding: value.binding,
            utf8,
            maximum_bytes: 4096,
            traps,
            cleanup: self.cleanup(None),
        });
        Ok(loan)
    }

    pub(super) fn copy(&mut self, expression: usize, value: &Value) -> Result<Value, BodyError> {
        let owner = self.owner(value, Category::Bytes, expression)?;
        self.steps.push(FlowStep::Copy {
            expression,
            owner,
            trap: TrapRequirement::PreservePrivatePreparationIdentity,
            cleanup: self.cleanup(None),
        });
        Ok(Value::plain(ValueType::VecI32))
    }
}
