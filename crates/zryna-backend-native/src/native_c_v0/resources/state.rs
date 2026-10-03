//! Per-function emission state, exact physical slots and checked calls to private helpers.

use super::{super::invariant_error, ledger};
use cranelift_codegen::{
    Context,
    ir::{
        FuncRef, InstBuilder, StackSlot, StackSlotData, StackSlotKind, Value, condcodes::IntCC,
        types,
    },
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use std::collections::BTreeMap;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::{
    VerifiedFunction, VerifiedMirProgram,
    contract::{FlowStep, PrivateOrigin},
};

pub(super) struct Environment<'a> {
    pub(super) program: &'a VerifiedMirProgram,
    pub(super) function: VerifiedFunction<'a>,
    pub(super) ordinal: usize,
    pub(super) imports: BTreeMap<usize, FuncRef>,
    pub(super) runtime: BTreeMap<String, FuncRef>,
    pub(super) byte_channel: bool,
    pub(super) releases: BTreeMap<usize, FuncRef>,
}

pub(super) struct State<'a, 'b> {
    pub(super) builder: FunctionBuilder<'a>,
    pub(super) environment: Environment<'b>,
    pub(super) context: Value,
    pub(super) inputs: Value,
    pub(super) outcome: Value,
    pub(super) frame: StackSlot,
    pub(super) values: Vec<Option<Value>>,
    pub(super) locals: Vec<Value>,
    pub(super) initialized: BTreeMap<usize, StackSlot>,
    pub(super) owners: BTreeMap<usize, StackSlot>,
    pub(super) owner_expected: BTreeMap<usize, StackSlot>,
    pub(super) owner_lengths: BTreeMap<usize, StackSlot>,
    pub(super) owner_pointers: BTreeMap<usize, StackSlot>,
    pub(super) private: BTreeMap<PrivateOrigin, (StackSlot, StackSlot)>,
    pub(super) loans: BTreeMap<usize, StackSlot>,
    pub(super) calls: BTreeMap<usize, (usize, Value)>,
    pub(super) snapshots: BTreeMap<usize, Vec<(usize, Value)>>,
    pub(super) cursor: usize,
    pub(super) terminated: bool,
}
impl<'a, 'b> State<'a, 'b> {
    pub(super) fn new(
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
    pub(super) fn constant(&mut self, value: usize) -> Result<Value, Diagnostic> {
        Ok(self
            .builder
            .ins()
            .iconst(types::I32, i64::from(u32::try_from(value).map_err(|_| invariant_error())?)))
    }
    pub(super) fn value(&self, expression: usize) -> Result<Value, Diagnostic> {
        self.values.get(expression).copied().flatten().ok_or_else(invariant_error)
    }
    pub(super) fn set(&mut self, expression: usize, value: Value) -> Result<(), Diagnostic> {
        let slot = self.values.get_mut(expression).ok_or_else(invariant_error)?;
        if slot.replace(value).is_some() {
            return Err(invariant_error());
        }
        Ok(())
    }
    pub(super) fn helper(&mut self, name: &str, arguments: &[Value]) -> Result<Value, Diagnostic> {
        let reference = *self.environment.runtime.get(name).ok_or_else(invariant_error)?;
        let call = self.builder.ins().call(reference, arguments);
        self.builder.inst_results(call).first().copied().ok_or_else(invariant_error)
    }
    pub(super) fn slot_address(&mut self, token: usize) -> Result<Value, Diagnostic> {
        let slot = self
            .environment
            .function
            .slots()
            .iter()
            .find(|slot| slot.token == token)
            .ok_or_else(invariant_error)?;
        Ok(self.builder.ins().stack_addr(
            types::I64,
            self.frame,
            i32::try_from(slot.offset).map_err(|_| invariant_error())?,
        ))
    }
    pub(super) fn slot_value(&mut self, token: usize) -> Result<Value, Diagnostic> {
        let slot = self
            .environment
            .function
            .slots()
            .iter()
            .find(|slot| slot.token == token)
            .ok_or_else(invariant_error)?;
        let ty = if slot.bytes == 4 { types::I32 } else { types::I64 };
        Ok(self.builder.ins().stack_load(
            types::I64,
            ty,
            self.frame,
            i32::try_from(slot.offset).map_err(|_| invariant_error())?,
        ))
    }
    pub(super) fn record(&mut self, owner: usize) -> Result<Value, Diagnostic> {
        let slot = *self.owners.get(&owner).ok_or_else(invariant_error)?;
        Ok(self.builder.ins().stack_load(types::I64, types::I64, slot, 0))
    }
    pub(super) fn finish(
        &mut self,
        tag: u8,
        operation: Option<usize>,
        status: Option<Value>,
        trap: u8,
        value: Option<Value>,
    ) -> Result<(), Diagnostic> {
        let tag = self.builder.ins().iconst(types::I32, i64::from(tag));
        self.finish_tag(tag, operation, status, trap, value, None)
    }
    pub(super) fn finish_tag(
        &mut self,
        tag: Value,
        operation: Option<usize>,
        status: Option<Value>,
        trap: u8,
        value: Option<Value>,
        protected_result: Option<PrivateOrigin>,
    ) -> Result<(), Diagnostic> {
        let operation = match operation {
            Some(id) => self.constant(id)?,
            None => self.builder.ins().iconst(types::I32, -1),
        };
        let zero = self.builder.ins().iconst(types::I32, 0);
        let trap = self.builder.ins().iconst(types::I32, i64::from(trap));
        let controlled = self.builder.ins().icmp_imm_s(IntCC::Equal, tag, 2);
        let trap = self.builder.ins().select(controlled, trap, zero);
        let exposed_scalar = if matches!(
            self.environment.function.result().1,
            zryna_native_mir::native_c_v0::contract::ValueType::VecI32
                | zryna_native_mir::native_c_v0::contract::ValueType::String
        ) {
            None
        } else {
            value
        };
        let private_count = super::storage::finish(self, tag, value, protected_result)?;
        let result = self.helper(
            ledger::FINISH,
            &[
                self.context,
                self.outcome,
                tag,
                operation,
                status.unwrap_or(zero),
                trap,
                exposed_scalar.unwrap_or(zero),
            ],
        )?;
        super::storage::unresolved(self, private_count);
        self.builder.ins().return_(&[result]);
        Ok(())
    }
    pub(super) fn handle_owner(&self, expression: usize) -> Result<usize, Diagnostic> {
        let token = self
            .environment
            .function
            .values()
            .get(expression)
            .and_then(|value| value.token)
            .ok_or_else(invariant_error)?;
        self.environment
            .function
            .effects()
            .find_map(|effect| {
                if let FlowStep::Take { expression, owner, .. } = effect.operation()
                    && self
                        .environment
                        .function
                        .values()
                        .get(*expression)
                        .is_some_and(|value| value.token == Some(token))
                {
                    return Some(*owner);
                }
                None
            })
            .ok_or_else(invariant_error)
    }
    pub(super) fn owner_arguments(
        &mut self,
        owner: usize,
        phase: u8,
    ) -> Result<Vec<Value>, Diagnostic> {
        let record = self.record(owner)?;
        let pointer_slot = *self.owner_pointers.get(&owner).ok_or_else(invariant_error)?;
        let pointer = self.builder.ins().stack_load(types::I64, types::I64, pointer_slot, 0);
        let function = self.constant(self.environment.ordinal)?;
        let owner_id = self.constant(owner)?;
        let release =
            super::admit::release_for(self.environment.function, self.environment.program, owner)
                .ok_or_else(invariant_error)?;
        let release = self.constant(release)?;
        let phase = self.builder.ins().iconst(types::I32, i64::from(phase));
        Ok(vec![self.context, record, pointer, function, owner_id, release, phase])
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
