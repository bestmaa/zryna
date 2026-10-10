//! Existing scalar and control-flow emission diagnostics, with unchanged codes and messages.

use zryna_diagnostics::Diagnostic;
use zryna_ir::{VerifiedFunction, control_flow_v1::FunctionIdentity};

pub(super) fn profile_invariant_error(function: VerifiedFunction<'_>) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W1001",
        None,
        format!(
            "verified function '{}' contains a type or operation outside the WebAssembly I32V1 proof profile",
            function.abi_export().webassembly_name().as_str()
        ),
        "report this compiler invariant failure with the smallest reproducible Zryna source",
    )
}

pub(super) fn index_error() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W1002",
        None,
        "verified WebAssembly indexes exceeded the core binary index space",
        "report this compiler invariant failure with the smallest reproducible Zryna source",
    )
}

pub(super) fn control_flow_profile_error(function: FunctionIdentity) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W2001",
        None,
        format!(
            "verified function {}:{} contains a type or identity outside the WebAssembly M2 proof profile",
            function.module().index(),
            function.declaration()
        ),
        "report this compiler invariant failure with the smallest reproducible Zryna source",
    )
}

pub(super) fn control_flow_index_error() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W2002",
        None,
        "verified M2 WebAssembly indexes exceeded the core binary index space",
        "report this compiler invariant failure with the smallest reproducible Zryna source",
    )
}

pub(super) fn control_flow_validation_error(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W2003",
        None,
        format!("emitted M2 core WebAssembly failed pinned WebAssembly 1.0 validation: {error}"),
        "report this compiler failure with the smallest reproducible Zryna source",
    )
}

pub(super) fn control_flow_audit_error(observation: &str) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W2004",
        None,
        format!("M2 core WebAssembly contains {observation}"),
        "emit only the exact sealed type, function, export, code, local, and instruction inventory",
    )
}

pub(super) fn control_flow_budget_error(limit: usize) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W2005",
        None,
        format!("deterministic M2 core WebAssembly exceeds its {limit} byte emission budget"),
        "reduce the verified ControlFlowV1 program below the WebAssembly artifact budget",
    )
}
