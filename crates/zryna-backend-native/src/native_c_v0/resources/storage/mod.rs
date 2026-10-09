//! Physical private storage under the retained layout/runtime issuer, never foreign ownership.

mod copy;
mod loans;
mod owners;
mod parameters;
mod terminal;
mod utf8;

use super::{super::invariant_error, state::State};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types};
use cranelift_codegen::isa::CallConv;
use cranelift_module::FuncId;
use cranelift_object::ObjectModule;
use std::collections::{BTreeMap, BTreeSet};
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::{
    VerifiedEffect, VerifiedMirProgram,
    contract::{BoundaryExitKind, PrivatePreparation, StorageStage, TrapRequirement},
};
use zryna_ownership_runtime_abi::{VerifiedNativeFunction, raw::NativeCarrier};

pub(super) use copy::{foreign_bytes, policy as bytes_policy, take};
pub(super) use loans::{boundary, byte_length, end_loans};
pub(super) use owners::{address, confirm_empty, finish, rejected, release, unresolved};
pub(super) use parameters::parameters;
pub(super) use terminal::drop as terminal_drop;
pub(super) const MAGIC: i64 = 0x5a43_4259_5445_5330;
pub(super) const UTF8: &str = "zryna_c_v0_r_utf8";

pub(super) fn enabled(program: &VerifiedMirProgram, selected: &[usize]) -> bool {
    program
        .functions()
        .enumerate()
        .any(|(index, function)| selected.contains(&index) && !function.private_owners().is_empty())
}

pub(super) fn runtime_imports(
    program: &VerifiedMirProgram,
    selected: &[usize],
) -> BTreeSet<String> {
    let mut required = Vec::new();
    for (index, function) in program.functions().enumerate() {
        if !selected.contains(&index) {
            continue;
        }
        required.extend(function.private_owners().iter().map(|owner| owner.release));
        for effect in function.effects() {
            match effect.preparation() {
                Some(PrivatePreparation::Loan(loan)) => required.extend(loan.allocation),
                Some(PrivatePreparation::Copy(copy)) => required.push(copy.allocation),
                None => {}
            }
        }
    }
    program
        .source()
        .runtime_abi()
        .native_linux_x86_64_functions()
        .filter(|function| required.contains(&function.operation()))
        .map(|function| function.symbol().to_owned())
        .collect()
}

pub(super) fn runtime_signature(
    function: VerifiedNativeFunction<'_>,
) -> Result<Signature, Diagnostic> {
    let mut signature = Signature::new(CallConv::SystemV);
    if function.result() != NativeCarrier::U32 {
        return Err(invariant_error());
    }
    signature.params.extend(function.parameters().iter().map(|carrier| {
        AbiParam::new(if *carrier == NativeCarrier::U32 { types::I32 } else { types::I64 })
    }));
    signature.returns.push(AbiParam::new(types::I32));
    Ok(signature)
}

pub(super) fn helper_names(program: &VerifiedMirProgram, selected: &[usize]) -> BTreeSet<String> {
    let foreign_utf8 = super::admit::imports(program, selected).iter().any(|index| {
        program.operations().nth(*index).is_some_and(|operation| {
            operation.declaration().resources.iter().any(|resource| {
                resource.encoding == zryna_native_mir::native_c_v0::contract::Encoding::Utf8
            })
        })
    });
    (foreign_utf8 || program.functions().enumerate().filter(|(index, _)| selected.contains(index))
        .flat_map(|(_, function)| function.effects()).any(|effect| {
            matches!(effect.preparation(), Some(PrivatePreparation::Loan(loan)) if loan.stages.contains(&StorageStage::Utf8))
        })).then(|| UTF8.to_owned()).into_iter().collect()
}

pub(super) fn define(
    program: &VerifiedMirProgram,
    selected: &[usize],
    object: &mut ObjectModule,
) -> Result<BTreeMap<String, FuncId>, Diagnostic> {
    let mut result = BTreeMap::new();
    if helper_names(program, selected).contains(UTF8) {
        result.insert(UTF8.to_owned(), utf8::define(object)?);
    }
    Ok(result)
}

pub(super) fn stage(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
    action: StorageStage,
) -> Result<(), Diagnostic> {
    match effect.preparation() {
        Some(PrivatePreparation::Loan(loan)) => loans::stage(state, effect, loan, action),
        Some(PrivatePreparation::Copy(copy)) => copy::stage(state, effect, copy, action),
        None => Err(invariant_error()),
    }
}

pub(super) fn trap_code(kind: &BoundaryExitKind) -> Result<u8, Diagnostic> {
    Ok(match kind {
        BoundaryExitKind::ForeignTrap(TrapRequirement::ForeignResourceLimit) => 1,
        BoundaryExitKind::ForeignTrap(TrapRequirement::ForeignLength) => 2,
        BoundaryExitKind::ForeignTrap(TrapRequirement::ForeignByteRange) => 3,
        BoundaryExitKind::PrivateTrap(fault) => match fault.declaration.trap_identity() {
            Some(zryna_ownership_runtime_abi::VerifiedStatusTrapIdentity::AllocationV1) => 4,
            Some(zryna_ownership_runtime_abi::VerifiedStatusTrapIdentity::CapacityV1) => 5,
            _ => return Err(invariant_error()),
        },
        BoundaryExitKind::ForeignTrap(TrapRequirement::PreservePrivatePreparationIdentity) => {
            return Err(invariant_error());
        }
        _ => 0,
    })
}

pub(super) fn allocation_status(
    state: &mut State<'_, '_>,
    effect: VerifiedEffect<'_>,
    status: cranelift_codegen::ir::Value,
) -> Result<(), Diagnostic> {
    let origin = match effect.preparation() {
        Some(PrivatePreparation::Loan(loan)) => loan.scratch.ok_or_else(invariant_error)?,
        Some(PrivatePreparation::Copy(copy)) => copy.result,
        None => return Err(invariant_error()),
    };
    let (handle, active) = *state.private.get(&origin).ok_or_else(invariant_error)?;
    let zero = state.builder.ins().iconst(types::I32, 0);
    let one = state.builder.ins().iconst(types::I32, 1);
    let success =
        state.builder.ins().icmp_imm_s(cranelift_codegen::ir::condcodes::IntCC::Equal, status, 0);
    let next = state.builder.create_block();
    let nonzero = state.builder.create_block();
    state.builder.ins().brif(success, next, &[], nonzero, &[]);
    state.builder.switch_to_block(nonzero);
    let mut known =
        state.builder.ins().icmp_imm_s(cranelift_codegen::ir::condcodes::IntCC::Equal, status, 255);
    for exit in effect.exits() {
        if let BoundaryExitKind::PrivateTrap(fault) = &exit.kind {
            let matches = state.builder.ins().icmp_imm_s(
                cranelift_codegen::ir::condcodes::IntCC::Equal,
                status,
                i64::from(fault.declaration.status() as u8),
            );
            known = state.builder.ins().bor(known, matches);
        }
    }
    let uncertain = state.builder.ins().select(known, zero, one);
    state.builder.ins().stack_store(types::I64, uncertain, active, 0);
    owners::host_check(state, known)?;
    let mut unchanged =
        state.builder.ins().icmp_imm_s(cranelift_codegen::ir::condcodes::IntCC::Equal, status, 0);
    for offset in [0, 8, 16] {
        let field = state.builder.ins().stack_load(types::I64, types::I64, handle, offset);
        let different = state.builder.ins().icmp_imm_s(
            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
            field,
            0,
        );
        unchanged = state.builder.ins().bor(unchanged, different);
    }
    let changed = state.builder.ins().uextend(types::I32, unchanged);
    state.builder.ins().stack_store(types::I64, changed, active, 0);
    let atomic =
        state.builder.ins().icmp_imm_s(cranelift_codegen::ir::condcodes::IntCC::Equal, changed, 0);
    owners::host_check(state, atomic)?;
    state.builder.ins().jump(next, &[]);
    state.builder.switch_to_block(next);
    for exit in effect.exits() {
        let (code, tag) = match &exit.kind {
            BoundaryExitKind::PrivateTrap(fault) => (fault.declaration.status() as u8, 2),
            BoundaryExitKind::PrivateAbiFailure(_) => (255, 3),
            _ => continue,
        };
        let different = state.builder.ins().icmp_imm_s(
            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
            status,
            i64::from(code),
        );
        super::cleanup::check_or_exit(
            state,
            different,
            effect,
            &exit.kind,
            tag,
            None,
            Some(status),
        )?;
    }
    let zero =
        state.builder.ins().icmp_imm_s(cranelift_codegen::ir::condcodes::IntCC::Equal, status, 0);
    owners::host_check(state, zero)
}

pub(super) fn header(header: &str) -> String {
    header.replace("0x5a4348414e444c30", "0x5a43425954455330").replace("struct zryna_c_v0_obligation owners[64]; };", "struct zryna_c_v0_obligation owners[64]; uint32_t private_unresolved, private_padding; };")
        .replace("sizeof(struct zryna_c_v0_context) == 1560", "sizeof(struct zryna_c_v0_context) == 1568")
        .replace("struct zryna_c_v0_inputs { uint32_t count, padding; uint32_t values[16]; };", "struct zryna_c_v0_storage { uint64_t pointer, length, capacity; };\nstruct zryna_c_v0_inputs { uint32_t count, padding; uint32_t values[16]; struct zryna_c_v0_storage owned[16]; };")
        .replace("uint32_t unresolved, reserved, padding; };", "uint32_t unresolved, reserved, padding; struct zryna_c_v0_storage owned; };")
        .replace("sizeof(struct zryna_c_v0_inputs) == 72", "sizeof(struct zryna_c_v0_inputs) == 456")
        .replace("sizeof(struct zryna_c_v0_outcome) == 32", "sizeof(struct zryna_c_v0_outcome) == 56")
        .replace("_Alignof(struct zryna_c_v0_inputs) == 4", "_Alignof(struct zryna_c_v0_inputs) == 8")
        .replace("_Alignof(struct zryna_c_v0_outcome) == 4", "_Alignof(struct zryna_c_v0_outcome) == 8")
        + "\n/* Private byte channel: owned inputs move after admission; owned result transfers only on Returned. */\n/* Trap codes: 1 FOREIGN_RESOURCE_LIMIT, 2 FOREIGN_LENGTH, 3 FOREIGN_BYTE_RANGE, 4 AllocationV1, 5 CapacityV1. */\n_Static_assert(sizeof(struct zryna_c_v0_storage) == 24 && offsetof(struct zryna_c_v0_inputs, owned) == 72 && offsetof(struct zryna_c_v0_outcome, owned) == 32 && offsetof(struct zryna_c_v0_context, private_unresolved) == 1560, \"private storage offsets\");\n"
}
