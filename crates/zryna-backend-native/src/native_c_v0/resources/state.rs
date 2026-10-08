//! Per-function emission state, exact physical slots and checked calls to private helpers.

use super::{super::invariant_error, ledger};
use cranelift_codegen::ir::{FuncRef, InstBuilder, StackSlot, Value, condcodes::IntCC, types};
use cranelift_frontend::FunctionBuilder;
use std::collections::BTreeMap;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::{
    VerifiedFunction, VerifiedMirProgram,
    contract::{FlowStep, PrivateOrigin},
};

mod initialization;

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
