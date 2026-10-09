//! Fresh output identity, exact successful initialization and paired linear takes.

use super::{BodyError, FailureRoute, FlowStep, Frame, Token, Value, ValueType, state::Output};
use zryna_syntax::native_c_v0::raw::Category;

impl Frame<'_> {
    pub(super) fn new_token(
        &mut self,
        ty: ValueType,
        kind: Option<String>,
        owner: Option<usize>,
    ) -> Value {
        let index = self.tokens.len();
        self.tokens.push(Token { ty, live: true, kind, output: None, owner });
        Value::token(ty, index)
    }

    pub(super) fn token(
        &self,
        value: &Value,
        ty: ValueType,
        expression: usize,
    ) -> Result<usize, BodyError> {
        let range = self.function.expressions[expression].range;
        let index = value.token.ok_or_else(|| self.type_error(range, "expected-foreign-token"))?;
        if value.ty != ty || self.tokens[index].ty != ty {
            return Err(self.type_error(range, "foreign-token-type"));
        }
        if !self.tokens[index].live {
            return Err(self.resource_error(range, "stale-token"));
        }
        Ok(index)
    }

    pub(super) fn output_slot(
        &mut self,
        expression: usize,
        ty: ValueType,
        kind: Option<String>,
    ) -> Value {
        let value = self.new_token(ty, kind, None);
        self.steps.push(FlowStep::OutputSlot { expression, token: self.tokens.len() - 1, ty });
        value
    }

    fn initialized(
        &self,
        value: &Value,
        ty: ValueType,
        expression: usize,
    ) -> Result<(usize, Output), BodyError> {
        let token = self.token(value, ty, expression)?;
        let range = self.function.expressions[expression].range;
        let output = self.tokens[token]
            .output
            .ok_or_else(|| self.resource_error(range, "output-before-call"))?;
        if !self.calls[output.call].proved_zero {
            return Err(self.resource_error(range, "output-before-exact-status-zero"));
        }
        Ok((token, output))
    }

    pub(super) fn read_i32(
        &mut self,
        expression: usize,
        value: &Value,
    ) -> Result<Value, BodyError> {
        let (slot, output) = self.initialized(value, ValueType::I32Out, expression)?;
        self.steps.push(FlowStep::ReadOutput { expression, slot, call: output.call });
        Ok(Value::plain(ValueType::I32))
    }

    pub(super) fn take_handle(
        &mut self,
        expression: usize,
        value: &Value,
    ) -> Result<Value, BodyError> {
        let (slot, output) = self.initialized(value, ValueType::HandleOut, expression)?;
        let owner = self.take_origin(expression, output, Category::Handle)?;
        self.transfer_owner(expression, owner, output.call, vec![slot], ValueType::Handle)
    }

    pub(super) fn take_bytes(
        &mut self,
        expression: usize,
        pointer: &Value,
        count: &Value,
    ) -> Result<Value, BodyError> {
        let (pointer, first) = self.initialized(pointer, ValueType::BytesOut, expression)?;
        let (count, second) = self.initialized(count, ValueType::CountOut, expression)?;
        if first.call != second.call || first.group != second.group || first.owner != second.owner {
            return Err(self.resource_error(
                self.function.expressions[expression].range,
                "mismatched-byte-output-pair",
            ));
        }
        let owner = self.take_origin(expression, first, Category::Bytes)?;
        self.transfer_owner(
            expression,
            owner,
            first.call,
            vec![pointer, count],
            ValueType::OwnedBytes,
        )
    }

    fn take_origin(
        &self,
        expression: usize,
        output: Output,
        category: Category,
    ) -> Result<usize, BodyError> {
        let range = self.function.expressions[expression].range;
        let owner =
            output.owner.ok_or_else(|| self.resource_error(range, "output-without-acquisition"))?;
        if self.owners[owner].category != category
            || !self.owners[owner].live
            || self.owners[owner].validated
        {
            return Err(self.resource_error(range, "already-taken-or-consumed-output"));
        }
        Ok(owner)
    }

    fn transfer_owner(
        &mut self,
        expression: usize,
        owner: usize,
        call: usize,
        slots: Vec<usize>,
        ty: ValueType,
    ) -> Result<Value, BodyError> {
        let entry = &self.owners[owner];
        if entry.slots != slots {
            return Err(self.resource_error(
                self.function.expressions[expression].range,
                "incomplete-owner-output-pair",
            ));
        }
        let kind = entry.kind.clone();
        let malformed_release_allowed = entry.releasable_on_malformed;
        let cleanup = self
            .cleanup(None)
            .into_iter()
            .filter(|entry| entry.owner != owner || malformed_release_allowed)
            .collect();
        // Acquisition already belongs to the call, before any validation/conversion/take.
        self.steps.push(FlowStep::Take {
            expression,
            owner,
            slots: slots.clone(),
            call,
            malformed_release_allowed,
            malformed_route: FailureRoute::HostAbiFailure,
            malformed_unresolved_owner: if malformed_release_allowed { None } else { Some(owner) },
            cleanup,
        });
        for slot in slots {
            self.tokens[slot].live = false;
        }
        self.owners[owner].validated = true;
        let value = self.new_token(ty, Some(kind), Some(owner));
        self.owners[owner].token = value.token;
        Ok(value)
    }
}
