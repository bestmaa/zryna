//! Exact resource-group origins are recorded before any take or output validation.

use super::{BodyError, BoundaryCheck, Frame, Owner, Value, ValueType, state::Output};
use zryna_syntax::native_c_v0::raw::{AbiType, Access, Category, Operation, Resource};

pub(super) struct Resources {
    pub created: Vec<usize>,
    pub consumed: Vec<usize>,
}

impl Frame<'_> {
    pub(super) fn plan_resources(
        &mut self,
        expression: usize,
        operation_index: usize,
        operation: &Operation,
        arguments: &[usize],
        values: &[Value],
        checks: &mut Vec<BoundaryCheck>,
    ) -> Result<Resources, BodyError> {
        let range = self.function.expressions[expression].range;
        let mut created = Vec::new();
        let mut consumed = Vec::new();
        for (group, resource) in operation.resources.iter().enumerate() {
            let first = usize::from(resource.slots[0]);
            match resource.access {
                Access::Read if operation.parameters[first].abi == AbiType::BytesIn => {
                    let loan = self.token(&values[first], ValueType::Bytes, expression)?;
                    let count = usize::from(resource.slots[1]);
                    checks.push(BoundaryCheck::Borrow {
                        group,
                        loan,
                        count_expression: arguments[count],
                        length_origin: values[count].length_of,
                    });
                }
                Access::Read => {
                    let owner = self.owner(&values[first], Category::Handle, expression)?;
                    self.nominal(owner, operation_index, resource, expression)?;
                }
                Access::Consume => {
                    let category = if operation.parameters[first].abi == AbiType::HandleIn {
                        Category::Handle
                    } else {
                        Category::Bytes
                    };
                    let owner = self.owner(&values[first], category, expression)?;
                    self.nominal(owner, operation_index, resource, expression)?;
                    if operation.key != self.owners[owner].release || consumed.contains(&owner) {
                        return Err(self.resource_error(range, "wrong-or-repeated-release-origin"));
                    }
                    consumed.push(owner);
                }
                Access::Create => {
                    created.push(self.acquire(expression, operation, group, resource, values)?);
                }
            }
        }
        Ok(Resources { created, consumed })
    }

    fn acquire(
        &mut self,
        expression: usize,
        operation: &Operation,
        group: usize,
        resource: &Resource,
        values: &[Value],
    ) -> Result<usize, BodyError> {
        let range = self.function.expressions[expression].range;
        let slots: Vec<_> = resource
            .slots
            .iter()
            .map(|slot| values[usize::from(*slot)].token)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| self.resource_error(range, "creation-output-identity"))?;
        let primary_slot = slots[0];
        let category =
            if operation.parameters[usize::from(resource.slots[0])].abi == AbiType::HandleOut {
                Category::Handle
            } else {
                Category::Bytes
            };
        if category == Category::Handle
            && self.tokens[primary_slot].kind.as_deref() != Some(resource.kind.as_str())
        {
            return Err(self.resource_error(range, "handle-output-nominal-kind"));
        }
        let owner = self.owners.len();
        let call = self.calls.len();
        self.owners.push(Owner {
            call,
            group,
            primary_slot,
            slots: slots.clone(),
            token: None,
            kind: resource.kind.clone(),
            library: operation.library.clone(),
            allocator: resource.allocator.clone(),
            release: resource.release.clone(),
            category,
            releasable_on_malformed: resource.releasable_on_malformed,
            live: true,
            validated: false,
        });
        for token in slots {
            self.tokens[token].output =
                Some(Output { call, group: Some(group), owner: Some(owner) });
        }
        Ok(owner)
    }
}
