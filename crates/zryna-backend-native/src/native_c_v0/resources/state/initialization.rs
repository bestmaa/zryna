//! Physical stack-frame allocation and initial per-function emission state.

use super::{Environment, State, invariant_error};
use cranelift_codegen::{
    Context,
    ir::{StackSlot, StackSlotData, StackSlotKind},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use std::collections::BTreeMap;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::{
    VerifiedFunction,
    contract::{FlowStep, PrivateOrigin},
};

impl<'a, 'b> State<'a, 'b> {
    pub(in super::super) fn new(
        environment: Environment<'b>,
        context: &'a mut Context,
        frontend: &'a mut FunctionBuilderContext,
    ) -> Result<Self, Diagnostic> {
        let mut builder = FunctionBuilder::new(&mut context.func, frontend);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let parameters = builder.block_params(entry).to_vec();
        let [context, inputs, outcome] = parameters.as_slice() else {
            return Err(invariant_error());
        };
        let context = *context;
        let inputs = *inputs;
        let outcome = *outcome;
        let frame = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            environment.function.output_frame_bytes().max(16),
            4,
        ));
        let mut initialized = BTreeMap::new();
        for slot in environment.function.slots() {
            initialized.insert(
                slot.token,
                builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    4,
                    2,
                )),
            );
        }
        let mut owners = BTreeMap::new();
        let mut owner_expected = BTreeMap::new();
        let mut owner_lengths = BTreeMap::new();
        let mut owner_pointers = BTreeMap::new();
        for effect in environment.function.effects() {
            if let FlowStep::Call { created_owners, .. } = effect.operation() {
                for owner in created_owners {
                    owner_expected.insert(
                        *owner,
                        builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        )),
                    );
                    owner_lengths.insert(
                        *owner,
                        builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        )),
                    );
                    owners.insert(
                        *owner,
                        builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        )),
                    );
                    owner_pointers.insert(
                        *owner,
                        builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        )),
                    );
                }
            }
        }
        let PrivateFrame { private, loans } = private_frame(environment.function, &mut builder);
        let values = vec![None; environment.function.values().len()];
        Ok(Self {
            builder,
            environment,
            context,
            inputs,
            outcome,
            frame,
            values,
            locals: Vec::new(),
            initialized,
            owners,
            owner_expected,
            owner_lengths,
            owner_pointers,
            private,
            loans,
            calls: BTreeMap::new(),
            snapshots: BTreeMap::new(),
            cursor: 0,
            terminated: false,
        })
    }
}

struct PrivateFrame {
    private: BTreeMap<PrivateOrigin, (StackSlot, StackSlot)>,
    loans: BTreeMap<usize, StackSlot>,
}
fn private_frame(
    function: VerifiedFunction<'_>,
    builder: &mut FunctionBuilder<'_>,
) -> PrivateFrame {
    let mut private = BTreeMap::new();
    for owner in function.private_owners() {
        let handle =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 24, 3));
        let active =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 4, 2));
        private.insert(owner.origin, (handle, active));
    }
    let mut loans = BTreeMap::new();
    for effect in function.effects() {
        if let FlowStep::PrepareLoan { token, .. } = effect.operation() {
            loans.insert(
                *token,
                builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    16,
                    3,
                )),
            );
        }
    }
    PrivateFrame { private, loans }
}
