//! Exact private clone labels and progress are observations, never grant authority.

use std::collections::BTreeMap;
use wasmparser::{BlockType, FunctionBody, Operator};
use zryna_diagnostics::Diagnostic;
use zryna_ir::{
    command_h1_v1::VerifiedProgram,
    data_ownership_v1::{VerifiedDropActionKind, VerifiedInstructionKind as K, VerifiedModule},
};
use zryna_layout::TypeCategory;

use super::{declarations::Shape, invalid};

mod nested_calls;
mod steps;
use steps::{audit_calls, audit_helper, audit_helper_calls, cover, step};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Step {
    Constant(i32),
    Get(u32),
    Set(u32),
    Call(u32),
    Tee(u32),
    Equal,
    If,
    End,
    Return,
    Branch(u32),
    Other,
}

#[derive(Clone, Copy)]
struct Binding {
    module: i32,
    declaration: i32,
    root: i32,
    helper: u32,
}

struct Callsite {
    helper: u32,
    binding: Option<Binding>,
}

pub(super) struct Protocol {
    calls: BTreeMap<u32, Vec<Callsite>>,
    helpers: BTreeMap<u32, (usize, bool)>,
    nested: nested_calls::References,
}

impl Protocol {
    pub(super) fn enabled(&self) -> bool {
        self.calls.values().flatten().any(|callsite| callsite.binding.is_some())
    }

    pub(super) fn derive(program: &VerifiedProgram, shape: &Shape) -> Result<Self, Diagnostic> {
        let mut calls = BTreeMap::new();
        for (ordinal, function) in program.modules().flat_map(VerifiedModule::functions).enumerate()
        {
            let mut bindings = Vec::new();
            for instruction in
                function.blocks().flat_map(zryna_ir::data_ownership_v1::VerifiedBlock::instructions)
            {
                if !matches!(
                    instruction.kind(),
                    K::ClonePlace
                        | K::GenericClonePlace
                        | K::GenericCloneBorrow
                        | K::HandleAwareClonePlace
                        | K::HandleAwareCloneBorrow
                        | K::StringClone
                        | K::VecClone
                        | K::SharedClone
                        | K::WeakClone
                        | K::WeakDowngrade
                ) {
                    continue;
                }
                let ty = instruction.result_type().ok_or_else(invalid)?;
                let helper = 6_u32.checked_add(ty.index()).ok_or_else(invalid)?;
                let actions = match instruction.kind() {
                    K::VecClone => {
                        instruction.vec_clone_element_failure_drop_actions().collect::<Vec<_>>()
                    }
                    K::ClonePlace => {
                        instruction.aggregate_clone_element_failure_drop_actions().collect()
                    }
                    K::GenericClonePlace | K::GenericCloneBorrow => {
                        instruction.generic_clone_prefix_failure_drop_actions().collect()
                    }
                    K::HandleAwareClonePlace | K::HandleAwareCloneBorrow => {
                        instruction.handle_aware_clone_prefix_failure_drop_actions().collect()
                    }
                    _ => Vec::new(),
                };
                let binding = if let Some(prefix) = actions.first() {
                    let expected_kind = match instruction.kind() {
                        K::VecClone => VerifiedDropActionKind::VecInitializedPrefix,
                        K::ClonePlace => VerifiedDropActionKind::AggregateInitializedPrefix,
                        _ => VerifiedDropActionKind::GenericCloneInitializedPrefix,
                    };
                    if prefix.kind() != expected_kind {
                        return Err(invalid());
                    }
                    Some(Binding {
                        module: i32::try_from(
                            0x2000_0000_u32
                                .checked_add(function.id().module())
                                .ok_or_else(invalid)?,
                        )
                        .map_err(|_| invalid())?,
                        declaration: i32::try_from(function.id().declaration())
                            .map_err(|_| invalid())?,
                        root: i32::try_from(prefix.root().index()).map_err(|_| invalid())?,
                        helper,
                    })
                } else {
                    None
                };
                bindings.push(Callsite { helper, binding });
            }
            calls.insert(
                shape.program_base + u32::try_from(ordinal).map_err(|_| invalid())?,
                bindings,
            );
        }
        Ok(Self {
            calls,
            helpers: helper_checks(program),
            nested: nested_calls::References::derive(program, shape)?,
        })
    }

    pub(super) fn audit(
        &self,
        body: &FunctionBody<'_>,
        index: u32,
        shape: &Shape,
    ) -> Result<(), Diagnostic> {
        let mut reader = body.get_operators_reader().map_err(|_| invalid())?;
        let mut steps = Vec::new();
        while !reader.eof() {
            steps.push(step(&reader.read().map_err(|_| invalid())?));
        }
        let mut covered = vec![false; steps.len()];
        if let Some(bindings) = self.calls.get(&index) {
            audit_calls(&steps, &mut covered, bindings, shape.type_count)?;
        } else if let Some((checks, acquired)) = self.helpers.get(&index) {
            if self.enabled() {
                audit_helper(&steps, &mut covered, *checks, *acquired, shape.run + 2)?;
            }
        } else if index == shape.run && self.enabled() {
            // The complete run audit independently fixes both reset positions.
            let resets = steps
                .windows(2)
                .enumerate()
                .filter(|(_, pair)| *pair == [Step::Constant(0), Step::Set(9)])
                .collect::<Vec<_>>();
            if resets.len() != 2 {
                return Err(invalid());
            }
            for (at, _) in resets {
                cover(&mut covered, at, 2)?;
            }
        }
        if index >= 6 && index < 6 + shape.type_count * 2 {
            audit_helper_calls(&steps, shape)?;
            self.nested.audit(body, index, self.enabled(), shape)?;
        } else if index > shape.run && steps.iter().any(|step| matches!(step, Step::Call(_))) {
            return Err(invalid());
        }
        if steps
            .iter()
            .zip(covered)
            .any(|(step, covered)| matches!(step, Step::Get(6..=9) | Step::Set(6..=9)) && !covered)
        {
            return Err(invalid());
        }
        Ok(())
    }
}

fn helper_checks(program: &VerifiedProgram) -> BTreeMap<u32, (usize, bool)> {
    let mut helpers = BTreeMap::new();
    let layouts = program.linear32_layouts();
    let owned = |id| layouts.type_by_id(id).is_some_and(|child| child.drop_kind() != 0);
    for ty in layouts.types() {
        let (checks, acquired) = match ty.category() {
            TypeCategory::Bool | TypeCategory::I32 => (0, false),
            TypeCategory::String | TypeCategory::Shared | TypeCategory::Weak => (2, false),
            TypeCategory::Struct => {
                (2 + ty.fields().iter().filter(|field| owned(field.ty())).count(), true)
            }
            TypeCategory::Enum => (
                2 + ty
                    .variants()
                    .iter()
                    .filter_map(|variant| variant.payload())
                    .filter(|id| owned(*id))
                    .count(),
                true,
            ),
            TypeCategory::FixedArray | TypeCategory::Vec => {
                (2 + usize::from(ty.referenced_type().is_some_and(owned)), true)
            }
        };
        helpers.insert(6 + ty.id().index(), (checks, acquired));
    }
    helpers
}
