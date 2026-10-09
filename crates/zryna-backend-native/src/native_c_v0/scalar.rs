//! Total scalar body and exact header projection; resource-bearing machine effects never enter.

use super::invariant_error;
use cranelift_codegen::{
    Context,
    ir::{AbiParam, InstBuilder, Signature, TrapCode, Value, condcodes::IntCC, types},
    isa::CallConv,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use std::fmt::Write;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::{
    VerifiedFunction, VerifiedMirProgram, abi,
    contract::{AbiType, StatementKind, ValueKind},
};

pub(super) fn signature(abi: &abi::Signature) -> Result<Signature, Diagnostic> {
    let mut signature = Signature::new(CallConv::SystemV);
    for lane in &abi.parameters {
        scalar_type(lane.abi)?;
        signature.params.push(AbiParam::new(types::I32));
    }
    scalar_type(abi.result.ok_or_else(invariant_error)?.abi)?;
    signature.returns.push(AbiParam::new(types::I32));
    Ok(signature)
}
fn scalar_type(ty: AbiType) -> Result<&'static str, Diagnostic> {
    match ty {
        AbiType::CI32 => Ok("int32_t"),
        AbiType::CInt => Ok("int"),
        AbiType::Bool32 => Ok("uint32_t"),
        _ => Err(invariant_error()),
    }
}

pub(super) fn header(program: &VerifiedMirProgram) -> Result<String, Diagnostic> {
    let mut header = String::from(
        "#ifndef ZRYNA_NATIVE_C_SCALAR_EXPORTS_V0_H\n#define ZRYNA_NATIVE_C_SCALAR_EXPORTS_V0_H\n\
         #include <stdint.h>\n\
         #if !defined(__linux__) || !defined(__x86_64__) || defined(__ILP32__)\n\
         #error Native C v0 exports require Linux x86-64 LP64\n\
         #endif\n\
         _Static_assert(sizeof(int32_t) == 4 && _Alignof(int32_t) == 4, \"i32 ABI\");\n\
         _Static_assert(sizeof(uint32_t) == 4 && _Alignof(uint32_t) == 4, \"bool32 ABI\");\n\
         _Static_assert(sizeof(int) == 4 && _Alignof(int) == 4, \"C int ABI\");\n\
         /* bool32 inputs require 0 or 1; invalid carriers terminate before the body. */\n",
    );
    let operations = program.operations().collect::<Vec<_>>();
    for function in program.functions() {
        let Some(export) = function.export() else { continue };
        let operation = operations.get(export).ok_or_else(invariant_error)?;
        let declaration = operation.declaration();
        write!(header, "{} {}(", scalar_type(declaration.result)?, declaration.symbol)
            .map_err(|_| invariant_error())?;
        if declaration.parameters.is_empty() {
            header.push_str("void");
        }
        for (index, parameter) in declaration.parameters.iter().enumerate() {
            if index > 0 {
                header.push_str(", ");
            }
            write!(header, "{} arg{index}", scalar_type(parameter.abi)?)
                .map_err(|_| invariant_error())?;
        }
        header.push_str(");\n");
    }
    header.push_str("#endif\n");
    Ok(header)
}

pub(super) fn build(
    function: VerifiedFunction<'_>,
    abi: &abi::Signature,
    context: &mut Context,
    builder_context: &mut FunctionBuilderContext,
    frontend_config: cranelift_codegen::isa::TargetFrontendConfig,
) -> Result<(), Diagnostic> {
    let mut builder = FunctionBuilder::new(&mut context.func, builder_context);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    let parameters = builder.block_params(entry).to_vec();
    // Check only meaningful low-width bits before evaluating any source operation.
    for (parameter, lane) in parameters.iter().zip(&abi.parameters) {
        if lane.canonical_bool {
            let invalid = builder.ins().icmp_imm_u(IntCC::UnsignedGreaterThan, *parameter, 1);
            builder.ins().trapnz(invalid, TrapCode::user(1).ok_or_else(invariant_error)?);
        }
    }
    let mut values = Vec::with_capacity(function.values().len());
    for definition in function.values() {
        let value = match definition.kind {
            ValueKind::I32(value) => builder.ins().iconst(types::I32, i64::from(value)),
            ValueKind::Bool(value) => builder.ins().iconst(types::I32, i64::from(value)),
            ValueKind::Local(index) => *parameters.get(index).ok_or_else(invariant_error)?,
            ValueKind::WrappingAdd(left, right) => {
                builder.ins().iadd(operand(&values, left)?, operand(&values, right)?)
            }
            ValueKind::Key(_) | ValueKind::Primitive(_, _) => return Err(invariant_error()),
        };
        values.push(value);
    }
    let [statement] = function.statements() else { return Err(invariant_error()) };
    let StatementKind::Return(expression) = statement.kind else { return Err(invariant_error()) };
    builder.ins().return_(&[operand(&values, expression)?]);
    builder.finalize(frontend_config);
    Ok(())
}
fn operand(values: &[Value], id: usize) -> Result<Value, Diagnostic> {
    values.get(id).copied().ok_or_else(invariant_error)
}
