//! Driver-owned total-scalar C export linking. No foreign library or private entry is selected.

use super::{
    ArtifactOutputRoot, LinuxX8664LinkToolchain, NativeProcessLimits, NativeRunError,
    PreparedNativeExecutable,
};
use zryna_abi::{ScalarOutcome, ScalarValue};
use zryna_backend_native::native_c_v0::ValidatedScalarExports;
use zryna_diagnostics::Diagnostic;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use {
    std::{fmt::Write as _, sync::Arc},
    zryna_native_mir::native_c_v0::contract::{AbiType, Direction},
};

/// Sealed scalar export invocation, retaining its exact complete source-to-machine authority.
#[derive(Clone, Debug)]
pub struct PreparedScalarExport {
    executable: PreparedNativeExecutable,
    object: ValidatedScalarExports,
}
impl PreparedScalarExport {
    /// Exact independently audited executable bytes, without publication authority.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.executable.bytes()
    }
    /// Original audited object and complete machine/source authority.
    #[must_use]
    pub const fn object(&self) -> &ValidatedScalarExports {
        &self.object
    }
    /// Runs the retained bytes through the existing bounded native process boundary.
    /// # Errors
    /// Reports process, framing or cleanup faults; process exit is never a scalar result.
    pub fn run(
        &self,
        root: &ArtifactOutputRoot,
        limits: NativeProcessLimits,
    ) -> Result<ScalarOutcome, NativeRunError> {
        super::run_prepared_native_invocation(&self.executable, root, limits)
    }
}

/// Validates an exact public C export and typed inputs, then stages, links and audits its invocation.
///
/// Source import names, compiler-private entry names and ambient libraries are never linker inputs.
/// Every input is checked before staging. The artifact remains unpublished; there is no CLI route.
/// # Errors
/// Rejects invalid export/arity/type, unsupported hosts, changed tools, link/audit or cleanup faults.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn prepare_scalar_export(
    object: &ValidatedScalarExports,
    logical: &str,
    arguments: &[ScalarValue],
    root: &ArtifactOutputRoot,
    toolchain: &LinuxX8664LinkToolchain,
    limits: NativeProcessLimits,
) -> Result<PreparedScalarExport, Vec<Diagnostic>> {
    let operation = object
        .program()
        .operations()
        .find(|operation| {
            let declaration = operation.declaration();
            declaration.direction == Direction::Export && declaration.logical_name == logical
        })
        .ok_or_else(|| vec![request_error()])?;
    let declaration = operation.declaration();
    if arguments.len() != declaration.parameters.len()
        || arguments.iter().zip(&declaration.parameters).any(|(value, parameter)| {
            !matches!(
                (value, parameter.abi),
                (ScalarValue::I32(_), AbiType::CI32 | AbiType::CInt)
                    | (ScalarValue::Bool(_), AbiType::Bool32)
            )
        })
    {
        return Err(vec![request_error()]);
    }
    let result_type = match declaration.result {
        AbiType::CI32 | AbiType::CInt => zryna_abi::ScalarType::I32,
        AbiType::Bool32 => zryna_abi::ScalarType::Bool,
        _ => return Err(vec![request_error()]),
    };
    // The exact selected prototype comes from the already generated full header. No complete
    // header is amplified into each invocation; other exports remain in the audited object.
    let prototype = object
        .header()
        .lines()
        .find(|line| {
            line.split_once(' ')
                .is_some_and(|(_, tail)| tail.starts_with(&format!("{}(", declaration.symbol)))
        })
        .ok_or_else(|| vec![request_error()])?;
    let mut harness = format!(
        "#include <stdint.h>\n#include <stdio.h>\n{prototype}\nint main(void) {{\nuint32_t value = (uint32_t){}(",
        declaration.symbol
    );
    for (index, value) in arguments.iter().enumerate() {
        if index > 0 {
            harness.push_str(", ");
        }
        match value {
            ScalarValue::I32(i32::MIN) => {
                harness.push_str("(-INT32_C(2147483647) - INT32_C(1))");
                Ok(())
            }
            ScalarValue::I32(value) => write!(harness, "INT32_C({value})"),
            ScalarValue::Bool(value) => write!(harness, "UINT32_C({})", u8::from(*value)),
        }
        .map_err(|_| vec![request_error()])?;
    }
    harness.push_str(");\nuint8_t frame[4] = {(uint8_t)value, (uint8_t)(value >> 8), (uint8_t)(value >> 16), (uint8_t)(value >> 24)};\nreturn fwrite(frame, 1, 4, stdout) == 4 ? 0 : 90;\n}\n");
    if harness.len() > super::MAX_NATIVE_HARNESS_BYTES {
        return Err(vec![request_error()]);
    }
    let (bytes, diagnostics) = super::link_and_audit_native_invocation(
        object.bytes(),
        harness.as_bytes(),
        &declaration.symbol,
        root,
        toolchain,
        limits,
    )?;
    if diagnostics.iter().any(|diagnostic| diagnostic.code() == "ZRYNA-N4016") {
        return Err(diagnostics);
    }
    Ok(PreparedScalarExport {
        executable: PreparedNativeExecutable {
            bytes: Arc::from(bytes),
            result_type,
            expected_symbol: declaration.symbol.clone().into_boxed_str(),
            ownership_identity: None,
            diagnostics,
        },
        object: object.clone(),
    })
}

/// Rejects native C export linking on hosts outside Linux x86-64 before staging.
/// # Errors
/// Always returns the unsupported-host category.
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub fn prepare_scalar_export(
    _object: &ValidatedScalarExports,
    _logical: &str,
    _arguments: &[ScalarValue],
    _root: &ArtifactOutputRoot,
    _toolchain: &LinuxX8664LinkToolchain,
    _limits: NativeProcessLimits,
) -> Result<PreparedScalarExport, Vec<Diagnostic>> {
    Err(vec![super::native_error(
        "ZRYNA-C4103",
        "native C export linking requires Linux x86-64",
        "use the exact accepted native C target and host",
    )])
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn request_error() -> Diagnostic {
    super::native_error(
        "ZRYNA-C4104",
        "native C export invocation does not match the retained scalar declaration",
        "select an exact public C export and supply its declared typed scalar arguments",
    )
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod tests;
