//! Text emission helpers for the verified MIR compatibility proof.

use super::verify_codegen_type;
use std::fmt::Write;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::{OperationView, ValueId, VerifiedCallingConvention, VerifiedMirFunction};

pub(super) fn emit_function(
    function: VerifiedMirFunction<'_>,
    output: &mut String,
) -> Result<(), Diagnostic> {
    match function.calling_convention() {
        VerifiedCallingConvention::ScalarAbiV1LinuxX8664SystemV => {}
    }
    verify_codegen_type(function.result_type())?;
    for ty in function.parameter_types() {
        verify_codegen_type(*ty)?;
    }
    write!(output, "define i32 @{}(", function.symbol()).map_err(native_format_error)?;
    for index in 0..function.parameter_types().len() {
        if index > 0 {
            output.push_str(", ");
        }
        write!(output, "i32 %p{index}").map_err(native_format_error)?;
    }
    output.push_str(") {\nentry:\n");
    for value in function.values() {
        verify_codegen_type(value.ty())?;
        let id = value.id().index();
        match value.operation() {
            OperationView::Parameter { .. } => {}
            OperationView::I32Literal { value } => {
                writeln!(output, "  %v{id} = add i32 0, {value}").map_err(native_format_error)?;
            }
            OperationView::I32Add { lhs, rhs } => {
                let left = llvm_value(function, lhs)?;
                let right = llvm_value(function, rhs)?;
                writeln!(output, "  %v{id} = add i32 {left}, {right}")
                    .map_err(native_format_error)?;
            }
        }
    }
    let result = llvm_value(function, function.result())?;
    write!(output, "  ret i32 {result}\n}}\n").map_err(native_format_error)?;
    Ok(())
}

fn llvm_value(function: VerifiedMirFunction<'_>, id: ValueId) -> Result<String, Diagnostic> {
    let value = function.value(id).ok_or_else(|| {
        Diagnostic::error(
            "ZRYNA-N2002",
            None,
            format!("verified native function '{}' references a missing value", function.symbol()),
            "report this compiler invariant failure with the smallest reproducible source",
        )
    })?;
    match value.operation() {
        OperationView::Parameter { index } => Ok(format!("%p{index}")),
        OperationView::I32Literal { .. } | OperationView::I32Add { .. } => {
            Ok(format!("%v{}", id.index()))
        }
    }
}

fn native_format_error(error: std::fmt::Error) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N2003",
        None,
        format!("native IR formatting failed: {error}"),
        "report this compiler failure with the smallest reproducible Zryna source",
    )
}
