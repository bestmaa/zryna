//! Verified native machine-independent representation.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use zryna_abi::{VerifiedScalarAbiModule, VerifiedScalarExport};
use zryna_diagnostics::Diagnostic;
use zryna_ir::{ExprKind, Type, UniversalProfile, VerifiedFunction, VerifiedProgram};

/// Separately versioned native MIR for the M2 structured control-flow profile.
pub mod control_flow_v1;
pub mod data_ownership_v1;
pub mod generic_copy_v1;
/// Maximum functions accepted in one native MIR module.
pub const MAX_MIR_FUNCTIONS: usize = 16_384;
/// Maximum parameters accepted in one native MIR function.
pub const MAX_MIR_PARAMETERS_PER_FUNCTION: usize = 256;
/// Maximum parameters accepted across one native MIR module.
pub const MAX_MIR_PARAMETERS_PER_MODULE: usize = 262_144;
/// Maximum value definitions accepted in one native MIR function.
pub const MAX_MIR_VALUES_PER_FUNCTION: usize = 16_384;
/// Maximum value definitions accepted across one native MIR module.
pub const MAX_MIR_VALUES_PER_MODULE: usize = 262_144;
/// Maximum bytes accepted in one provisional native symbol input.
pub const MAX_MIR_SYMBOL_BYTES: usize = 128;
/// Maximum symbol bytes accepted across one native MIR module.
pub const MAX_MIR_SYMBOL_BYTES_PER_MODULE: usize = MAX_MIR_FUNCTIONS * MAX_MIR_SYMBOL_BYTES;
/// Maximum retained MIR diagnostics, including the terminal budget diagnostic.
pub const MAX_MIR_DIAGNOSTICS: usize = 256;

const _: () = {
    assert!(MAX_MIR_FUNCTIONS >= zryna_ir::MAX_IR_FUNCTIONS);
    assert!(MAX_MIR_PARAMETERS_PER_FUNCTION >= zryna_ir::MAX_IR_PARAMETERS_PER_FUNCTION);
    assert!(MAX_MIR_PARAMETERS_PER_MODULE >= zryna_ir::MAX_IR_PARAMETERS_PER_PROGRAM);
    assert!(MAX_MIR_VALUES_PER_FUNCTION >= zryna_ir::MAX_IR_EXPRESSIONS_PER_FUNCTION);
    assert!(MAX_MIR_VALUES_PER_MODULE >= zryna_ir::MAX_IR_EXPRESSIONS_PER_PROGRAM);
    assert!(MAX_MIR_SYMBOL_BYTES >= zryna_ir::MAX_IR_EXPORT_NAME_BYTES);
};

/// Raw native MIR scalar type claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MirType {
    /// Reserved no-value type.
    Unit,
    /// Reserved boolean type.
    Bool,
    /// Signed wrapping 32-bit integer.
    I32,
}

/// Explicit untrusted native MIR claims.
///
/// Constructors in this module validate only Rust-level shape. Values remain untrusted until
/// consumed by [`crate::verify`], and native code generation does not accept these types.
pub mod raw {
    use super::MirType;

    /// Untrusted function-local value identifier.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ValueId(u32);

    impl ValueId {
        /// Creates an unverified raw value identifier.
        #[must_use]
        pub const fn new(index: u32) -> Self {
            Self(index)
        }

        /// Returns the claimed dense value index.
        #[must_use]
        pub const fn index(self) -> u32 {
            self.0
        }
    }

    /// Untrusted calling-convention claim.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CallingConvention(u16);

    impl CallingConvention {
        /// Provisional internal convention for the straight-line `i32` proof.
        ///
        /// This is not the public scalar ABI or an FFI contract.
        pub const ZRYNA_INTERNAL_I32_V1: Self = Self(1);

        /// Creates an unverified convention code for provider and negative-test inputs.
        #[must_use]
        pub const fn from_code(code: u16) -> Self {
            Self(code)
        }

        /// Returns the raw convention code.
        #[must_use]
        pub const fn code(self) -> u16 {
            self.0
        }
    }

    /// Untrusted native MIR function signature.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Signature {
        pub(super) parameters: Vec<MirType>,
        pub(super) result: MirType,
    }

    impl Signature {
        /// Creates an unverified signature claim.
        #[must_use]
        pub const fn new(parameters: Vec<MirType>, result: MirType) -> Self {
            Self { parameters, result }
        }
    }

    /// Untrusted native MIR operation claim.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum Operation {
        /// Read a function argument.
        Parameter {
            /// Claimed zero-based parameter index.
            index: u32,
        },
        /// Create a signed 32-bit literal.
        I32Literal {
            /// Literal value.
            value: i32,
        },
        /// Add two signed 32-bit values with wrapping semantics.
        I32Add {
            /// Claimed left value.
            lhs: ValueId,
            /// Claimed right value.
            rhs: ValueId,
        },
    }

    /// Untrusted typed value definition.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ValueDefinition {
        pub(super) id: ValueId,
        pub(super) ty: MirType,
        pub(super) operation: Operation,
    }

    impl ValueDefinition {
        /// Creates an unverified typed value definition.
        #[must_use]
        pub const fn new(id: ValueId, ty: MirType, operation: Operation) -> Self {
            Self { id, ty, operation }
        }
    }

    /// Untrusted native MIR function.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Function {
        pub(super) symbol: String,
        pub(super) convention: CallingConvention,
        pub(super) signature: Signature,
        pub(super) values: Vec<ValueDefinition>,
        pub(super) result: ValueId,
    }

    impl Function {
        /// Creates an unverified native function claim.
        #[must_use]
        pub const fn new(
            symbol: String,
            convention: CallingConvention,
            signature: Signature,
            values: Vec<ValueDefinition>,
            result: ValueId,
        ) -> Self {
            Self { symbol, convention, signature, values, result }
        }
    }

    /// Untrusted native MIR module.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Module {
        pub(super) functions: Vec<Function>,
    }

    impl Module {
        /// Creates an unverified native module claim.
        #[must_use]
        pub const fn new(functions: Vec<Function>) -> Self {
            Self { functions }
        }
    }
}

/// Opaque function-local value proven by the native MIR verifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueId(u32);

impl ValueId {
    /// Returns the verified dense value index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Sealed calling convention admitted by the current native MIR proof profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedCallingConvention {
    /// Scalar ABI v1 on Linux x86-64 using the System V AMD64 calling convention.
    ScalarAbiV1LinuxX8664SystemV,
}

/// Read-only view of one verified native MIR operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationView {
    /// Read a function argument.
    Parameter {
        /// Verified zero-based parameter index.
        index: u32,
    },
    /// Create a signed 32-bit literal.
    I32Literal {
        /// Literal value.
        value: i32,
    },
    /// Add two earlier signed 32-bit values with wrapping semantics.
    I32Add {
        /// Verified left predecessor.
        lhs: ValueId,
        /// Verified right predecessor.
        rhs: ValueId,
    },
}

/// Native MIR module proven safe for the current native backend proof.
///
/// Construction is reserved to [`verify`]:
///
/// ```compile_fail
/// let raw = zryna_native_mir::raw::Module::new(Vec::new());
/// let _ = zryna_native_mir::VerifiedMirModule { raw };
/// ```
#[derive(Clone, Debug)]
pub struct VerifiedMirModule {
    raw: raw::Module,
    abi: VerifiedScalarAbiModule,
}

impl VerifiedMirModule {
    /// Iterates immutable verified function views in module order.
    #[must_use]
    pub fn functions(&self) -> impl ExactSizeIterator<Item = VerifiedMirFunction<'_>> {
        self.raw
            .functions
            .iter()
            .zip(self.abi.exports())
            .map(|(function, abi_export)| VerifiedMirFunction { raw: function, abi_export })
    }
}

/// Immutable view of one verified native MIR function.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedMirFunction<'module> {
    raw: &'module raw::Function,
    abi_export: VerifiedScalarExport<'module>,
}

impl<'module> VerifiedMirFunction<'module> {
    /// Returns the exact verified Linux x86-64 scalar ABI v1 symbol.
    #[must_use]
    pub fn symbol(self) -> &'module str {
        self.abi_export.native_linux_x86_64_symbol().as_str()
    }

    /// Returns the scalar ABI authority retained by native MIR verification.
    #[must_use]
    pub const fn abi_export(self) -> VerifiedScalarExport<'module> {
        self.abi_export
    }

    /// Returns the verified public calling convention.
    #[must_use]
    pub const fn calling_convention(self) -> VerifiedCallingConvention {
        VerifiedCallingConvention::ScalarAbiV1LinuxX8664SystemV
    }

    /// Returns the verified parameter types.
    #[must_use]
    pub fn parameter_types(self) -> &'module [MirType] {
        &self.raw.signature.parameters
    }

    /// Returns the verified result type.
    #[must_use]
    pub const fn result_type(self) -> MirType {
        self.raw.signature.result
    }

    /// Iterates verified value definitions in canonical dense order.
    #[must_use]
    pub fn values(self) -> impl ExactSizeIterator<Item = VerifiedValue<'module>> {
        self.raw.values.iter().map(|raw| VerifiedValue { raw })
    }

    /// Finds a verified value by its opaque function-local identifier.
    #[must_use]
    pub fn value(self, id: ValueId) -> Option<VerifiedValue<'module>> {
        usize::try_from(id.0)
            .ok()
            .and_then(|index| self.raw.values.get(index))
            .map(|raw| VerifiedValue { raw })
    }

    /// Returns the verified result value.
    #[must_use]
    pub const fn result(self) -> ValueId {
        ValueId(self.raw.result.index())
    }
}

/// Immutable view of one verified native MIR value definition.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedValue<'function> {
    raw: &'function raw::ValueDefinition,
}

impl VerifiedValue<'_> {
    /// Returns the verified dense value identifier.
    #[must_use]
    pub const fn id(self) -> ValueId {
        ValueId(self.raw.id.index())
    }

    /// Returns the verified value type.
    #[must_use]
    pub const fn ty(self) -> MirType {
        self.raw.ty
    }

    /// Returns a copyable read-only operation view.
    #[must_use]
    pub const fn operation(self) -> OperationView {
        match self.raw.operation {
            raw::Operation::Parameter { index } => OperationView::Parameter { index },
            raw::Operation::I32Literal { value } => OperationView::I32Literal { value },
            raw::Operation::I32Add { lhs, rhs } => {
                OperationView::I32Add { lhs: ValueId(lhs.index()), rhs: ValueId(rhs.index()) }
            }
        }
    }
}

/// Verifies untrusted native MIR and consumes it into an immutable backend-safe wrapper.
///
/// Verification is iterative, deterministic, and bounded. The current proof profile accepts only
/// dense straight-line `i32` SSA values and one provisional internal calling convention.
///
/// # Errors
///
/// Returns bounded stable diagnostics when any symbol, signature, value, graph, result, convention,
/// or resource invariant is unproven.
pub fn verify(module: raw::Module) -> Result<VerifiedMirModule, Vec<Diagnostic>> {
    let mut errors = VerificationErrors::default();
    verify_resource_limits(&module, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    verify_symbols(&module, &mut errors);
    for (function_index, function) in module.functions.iter().enumerate() {
        if errors.exhausted() {
            break;
        }
        verify_function(function_index, function, &mut errors);
    }
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let abi = verify_scalar_abi(&module)?;
    Ok(VerifiedMirModule { raw: module, abi })
}

fn verify_scalar_abi(module: &raw::Module) -> Result<VerifiedScalarAbiModule, Vec<Diagnostic>> {
    let exports = module
        .functions
        .iter()
        .map(|function| {
            let parameters =
                function.signature.parameters.iter().map(|_| zryna_abi::raw::Type::I32).collect();
            zryna_abi::raw::Export::new(
                function.symbol.clone(),
                zryna_abi::raw::Signature::new(parameters, zryna_abi::raw::Type::I32),
            )
        })
        .collect();
    zryna_abi::verify_v1(zryna_abi::raw::Module::new(exports)).map_err(|_| {
        vec![Diagnostic::error(
            "ZRYNA-N1013",
            None,
            "verified native MIR could not seal scalar ABI v1",
            "report this compiler invariant failure with the smallest reproducible source",
        )]
    })
}

/// Lowers verified target-neutral IR through the mandatory native MIR verifier.
///
/// Raw Universal IR cannot enter native lowering:
///
/// ```compile_fail
/// let raw = zryna_ir::Program::default();
/// let _ = zryna_native_mir::lower(&raw);
/// ```
///
/// Raw `ControlFlowV1` claims cannot satisfy the existing native lowering boundary:
///
/// ```compile_fail
/// let raw = zryna_ir::control_flow_v1::raw::Program {
///     entry_module: zryna_ir::control_flow_v1::raw::ModuleId(0),
///     modules: Vec::new(),
/// };
/// let _ = zryna_native_mir::lower(&raw);
/// ```
///
/// # Errors
///
/// Returns diagnostics if a future Universal IR profile cannot map to the current native proof or
/// if native MIR verification exposes an internal lowering invariant failure.
pub fn lower(program: &VerifiedProgram) -> Result<VerifiedMirModule, Vec<Diagnostic>> {
    match program.profile() {
        UniversalProfile::I32V1 => {}
    }
    let mut functions = Vec::new();
    for function in program.functions() {
        match lower_function(function) {
            Ok(function) => functions.push(function),
            Err(diagnostic) => return Err(vec![diagnostic]),
        }
    }
    verify(raw::Module::new(functions))
}

fn lower_function(function: VerifiedFunction<'_>) -> Result<raw::Function, Diagnostic> {
    let parameters =
        function.parameters().iter().copied().map(lower_type).collect::<Result<Vec<_>, _>>()?;
    let result_type = lower_type(function.return_type())?;
    let mut values = Vec::with_capacity(function.expressions().len());
    for (index, expression) in function.expressions().iter().enumerate() {
        let id = u32::try_from(index).map_err(native_lowering_error)?;
        let ty = lower_type(expression.ty)?;
        let operation = match &expression.kind {
            ExprKind::Parameter(index) => raw::Operation::Parameter { index: *index },
            ExprKind::BoolLiteral(_) => {
                return Err(Diagnostic::error(
                    "ZRYNA-N1001",
                    None,
                    "verified Universal IR contains an operation outside the native proof profile",
                    "report this compiler invariant failure with the smallest reproducible source",
                ));
            }
            ExprKind::I32Literal(value) => raw::Operation::I32Literal { value: *value },
            ExprKind::I32Add { lhs, rhs } => raw::Operation::I32Add {
                lhs: raw::ValueId::new(lhs.0),
                rhs: raw::ValueId::new(rhs.0),
            },
        };
        values.push(raw::ValueDefinition::new(raw::ValueId::new(id), ty, operation));
    }
    Ok(raw::Function::new(
        function.abi_export().logical_name().as_str().to_owned(),
        raw::CallingConvention::ZRYNA_INTERNAL_I32_V1,
        raw::Signature::new(parameters, result_type),
        values,
        raw::ValueId::new(function.body().0),
    ))
}

fn lower_type(ty: Type) -> Result<MirType, Diagnostic> {
    match ty {
        Type::I32 => Ok(MirType::I32),
        Type::Unit | Type::Bool => Err(Diagnostic::error(
            "ZRYNA-N1001",
            None,
            "verified Universal IR contains a type outside the native proof profile",
            "report this compiler invariant failure with the smallest reproducible source",
        )),
    }
}

fn native_lowering_error(error: std::num::TryFromIntError) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N1001",
        None,
        format!("verified Universal IR exceeds the native value identifier range: {error}"),
        "report this compiler invariant failure with the smallest reproducible source",
    )
}

fn verify_resource_limits(module: &raw::Module, errors: &mut VerificationErrors) {
    if module.functions.len() > MAX_MIR_FUNCTIONS {
        errors.push(limit_error("function count", MAX_MIR_FUNCTIONS));
        return;
    }
    let mut parameters = 0_usize;
    let mut values = 0_usize;
    let mut symbol_bytes = 0_usize;
    for (function_index, function) in module.functions.iter().enumerate() {
        if function.signature.parameters.len() > MAX_MIR_PARAMETERS_PER_FUNCTION {
            errors.push(function_limit_error(
                function_index,
                "parameters",
                MAX_MIR_PARAMETERS_PER_FUNCTION,
            ));
        }
        if function.values.len() > MAX_MIR_VALUES_PER_FUNCTION {
            errors.push(function_limit_error(
                function_index,
                "values",
                MAX_MIR_VALUES_PER_FUNCTION,
            ));
        }
        if function.symbol.len() > MAX_MIR_SYMBOL_BYTES {
            errors.push(function_limit_error(function_index, "symbol bytes", MAX_MIR_SYMBOL_BYTES));
        }
        let Some(parameter_total) = parameters.checked_add(function.signature.parameters.len())
        else {
            errors.push(limit_error("aggregate parameter count", MAX_MIR_PARAMETERS_PER_MODULE));
            return;
        };
        parameters = parameter_total;
        let Some(value_total) = values.checked_add(function.values.len()) else {
            errors.push(limit_error("aggregate value count", MAX_MIR_VALUES_PER_MODULE));
            return;
        };
        values = value_total;
        let Some(symbol_total) = symbol_bytes.checked_add(function.symbol.len()) else {
            errors.push(limit_error("aggregate symbol bytes", MAX_MIR_SYMBOL_BYTES_PER_MODULE));
            return;
        };
        symbol_bytes = symbol_total;
    }
    if parameters > MAX_MIR_PARAMETERS_PER_MODULE {
        errors.push(limit_error("aggregate parameter count", MAX_MIR_PARAMETERS_PER_MODULE));
    }
    if values > MAX_MIR_VALUES_PER_MODULE {
        errors.push(limit_error("aggregate value count", MAX_MIR_VALUES_PER_MODULE));
    }
    if symbol_bytes > MAX_MIR_SYMBOL_BYTES_PER_MODULE {
        errors.push(limit_error("aggregate symbol bytes", MAX_MIR_SYMBOL_BYTES_PER_MODULE));
    }
}

fn limit_error(label: &str, limit: usize) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N1201",
        None,
        format!("native MIR {label} exceeds its limit of {limit}"),
        "reduce the module before native MIR verification",
    )
}

fn function_limit_error(function_index: usize, label: &str, limit: usize) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N1201",
        None,
        format!("native function #{function_index} has too many {label}; the limit is {limit}"),
        "reduce the function before native MIR verification",
    )
}

fn verify_symbols(module: &raw::Module, errors: &mut VerificationErrors) {
    let mut exact = BTreeMap::<String, usize>::new();
    let mut portable = BTreeMap::<String, usize>::new();
    for (function_index, function) in module.functions.iter().enumerate() {
        if !valid_symbol(&function.symbol) {
            errors.push(Diagnostic::error(
                "ZRYNA-N1002",
                None,
                format!("native function #{function_index} has an invalid provisional symbol"),
                "use bounded ASCII [A-Za-z_][A-Za-z0-9_]* without reserved bindings",
            ));
            continue;
        }
        if let Some(previous) = exact.get(&function.symbol).copied() {
            errors.push(Diagnostic::error(
                "ZRYNA-N1003",
                None,
                format!(
                    "native function #{function_index} duplicates the symbol of function #{previous}"
                ),
                "give every native function one exact unique provisional symbol",
            ));
            continue;
        }
        exact.insert(function.symbol.clone(), function_index);
        let identity = function.symbol.to_ascii_lowercase();
        if let Some(previous) = portable.get(&identity).copied() {
            errors.push(Diagnostic::error(
                "ZRYNA-N1003",
                None,
                format!(
                    "native function #{function_index} collides with function #{previous} under the portable symbol identity"
                ),
                "use symbols that remain unique when ASCII case is ignored",
            ));
        } else {
            portable.insert(identity, function_index);
        }
    }
}

fn valid_symbol(symbol: &str) -> bool {
    let mut bytes = symbol.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && !reserved_symbol(symbol)
}

fn reserved_symbol(symbol: &str) -> bool {
    matches!(
        symbol,
        "arguments"
            | "await"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "constructor"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "eval"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "implements"
            | "import"
            | "in"
            | "instanceof"
            | "interface"
            | "let"
            | "new"
            | "null"
            | "package"
            | "private"
            | "protected"
            | "prototype"
            | "public"
            | "return"
            | "static"
            | "super"
            | "switch"
            | "then"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "__proto__"
    )
}

fn verify_function(
    function_index: usize,
    function: &raw::Function,
    errors: &mut VerificationErrors,
) {
    if function.convention != raw::CallingConvention::ZRYNA_INTERNAL_I32_V1 {
        errors.push(Diagnostic::error(
            "ZRYNA-N1004",
            None,
            format!(
                "native function #{function_index} uses unsupported convention code {}",
                function.convention.code()
            ),
            "use the provisional internal i32 convention for the current native proof",
        ));
    }
    for (parameter_index, ty) in function.signature.parameters.iter().enumerate() {
        if *ty != MirType::I32 {
            errors.push(unsupported_signature_error(function_index, "parameter", parameter_index));
        }
    }
    if function.signature.result != MirType::I32 {
        errors.push(Diagnostic::error(
            "ZRYNA-N1005",
            None,
            format!("native function #{function_index} has an unsupported result type"),
            "use only i32 in the current native MIR proof profile",
        ));
    }

    let definitions_are_dense = verify_definitions(function_index, function, errors);
    verify_operations(function_index, function, errors);
    verify_result(function_index, function, errors);
    if definitions_are_dense {
        verify_cycles(function_index, function, errors);
    }
}

fn unsupported_signature_error(
    function_index: usize,
    label: &str,
    item_index: usize,
) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N1005",
        None,
        format!("native function #{function_index} {label} #{item_index} has an unsupported type"),
        "use only i32 in the current native MIR proof profile",
    )
}

fn verify_definitions(
    function_index: usize,
    function: &raw::Function,
    errors: &mut VerificationErrors,
) -> bool {
    let mut dense = true;
    for (value_index, value) in function.values.iter().enumerate() {
        let expected = u32::try_from(value_index).ok();
        if expected != Some(value.id.index()) {
            dense = false;
            errors.push(Diagnostic::error(
                "ZRYNA-N1006",
                None,
                format!(
                    "native function #{function_index} value slot #{value_index} claims a non-dense or duplicate identifier"
                ),
                "define every value exactly once with its canonical dense slot identifier",
            ));
        }
    }
    dense
}

fn verify_operations(
    function_index: usize,
    function: &raw::Function,
    errors: &mut VerificationErrors,
) {
    for (value_index, value) in function.values.iter().enumerate() {
        if errors.exhausted() {
            return;
        }
        match value.operation {
            raw::Operation::Parameter { index } => {
                let parameter = usize::try_from(index)
                    .ok()
                    .and_then(|index| function.signature.parameters.get(index));
                if parameter != Some(&value.ty) {
                    errors.push(Diagnostic::error(
                        "ZRYNA-N1010",
                        None,
                        format!(
                            "native function #{function_index} value #{value_index} references an invalid parameter"
                        ),
                        "use an existing parameter index with the exact value type",
                    ));
                }
            }
            raw::Operation::I32Literal { .. } => {
                if value.ty != MirType::I32 {
                    errors.push(operation_type_error(function_index, value_index));
                }
            }
            raw::Operation::I32Add { lhs, rhs } => {
                let left = verify_operand(function_index, value_index, lhs, function, errors);
                let right = verify_operand(function_index, value_index, rhs, function, errors);
                let operands_are_i32 = left.is_some_and(|ty| ty == MirType::I32)
                    && right.is_some_and(|ty| ty == MirType::I32);
                if value.ty != MirType::I32 || !operands_are_i32 {
                    errors.push(operation_type_error(function_index, value_index));
                }
            }
        }
    }
}

fn verify_operand(
    function_index: usize,
    value_index: usize,
    operand: raw::ValueId,
    function: &raw::Function,
    errors: &mut VerificationErrors,
) -> Option<MirType> {
    let Some(operand_index) = usize::try_from(operand.index()).ok() else {
        errors.push(missing_value_error(function_index, value_index, "operand"));
        return None;
    };
    let Some(definition) = function.values.get(operand_index) else {
        errors.push(missing_value_error(function_index, value_index, "operand"));
        return None;
    };
    if definition.id.index() != operand.index() {
        errors.push(missing_value_error(function_index, value_index, "operand"));
        return None;
    }
    if operand_index >= value_index {
        errors.push(Diagnostic::error(
            "ZRYNA-N1008",
            None,
            format!(
                "native function #{function_index} value #{value_index} references a non-predecessor operand"
            ),
            "reference only values defined in an earlier canonical slot",
        ));
    }
    Some(definition.ty)
}

fn missing_value_error(function_index: usize, value_index: usize, label: &str) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N1007",
        None,
        format!("native function #{function_index} value #{value_index} has a missing {label}"),
        "reference a uniquely defined value in the same native function",
    )
}

fn operation_type_error(function_index: usize, value_index: usize) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N1011",
        None,
        format!("native function #{function_index} value #{value_index} has an invalid type"),
        "make the value and every operation input match the exact operation type",
    )
}

fn verify_result(function_index: usize, function: &raw::Function, errors: &mut VerificationErrors) {
    let result = usize::try_from(function.result.index())
        .ok()
        .and_then(|index| function.values.get(index))
        .filter(|definition| definition.id.index() == function.result.index());
    let Some(result) = result else {
        errors.push(Diagnostic::error(
            "ZRYNA-N1007",
            None,
            format!("native function #{function_index} has a missing result value"),
            "return one uniquely defined value from the same native function",
        ));
        return;
    };
    if result.ty != function.signature.result {
        errors.push(Diagnostic::error(
            "ZRYNA-N1012",
            None,
            format!("native function #{function_index} returns the wrong MIR type"),
            "make the result value type equal the declared signature result type",
        ));
    }
}

fn verify_cycles(function_index: usize, function: &raw::Function, errors: &mut VerificationErrors) {
    let mut colors = vec![0_u8; function.values.len()];
    for start in 0..function.values.len() {
        if colors[start] != 0 {
            continue;
        }
        colors[start] = 1;
        let mut stack = vec![(start, 0_u8)];
        while let Some((node, next_edge)) = stack.last_mut() {
            let operands = operation_operands(&function.values[*node].operation);
            let Some(operand) = operands.get(usize::from(*next_edge)).copied().flatten() else {
                colors[*node] = 2;
                stack.pop();
                continue;
            };
            *next_edge = next_edge.saturating_add(1);
            let Some(operand_index) = usize::try_from(operand.index())
                .ok()
                .filter(|index| *index < function.values.len())
            else {
                continue;
            };
            match colors[operand_index] {
                0 => {
                    colors[operand_index] = 1;
                    stack.push((operand_index, 0));
                }
                1 => {
                    errors.push(Diagnostic::error(
                        "ZRYNA-N1009",
                        None,
                        format!("native function #{function_index} contains a value cycle"),
                        "make every value graph acyclic and reference only predecessors",
                    ));
                    return;
                }
                _ => {}
            }
        }
    }
}

fn operation_operands(operation: &raw::Operation) -> [Option<raw::ValueId>; 2] {
    match operation {
        raw::Operation::Parameter { .. } | raw::Operation::I32Literal { .. } => [None, None],
        raw::Operation::I32Add { lhs, rhs } => [Some(*lhs), Some(*rhs)],
    }
}

#[derive(Default)]
struct VerificationErrors {
    diagnostics: Vec<Diagnostic>,
    exhausted: bool,
}

impl VerificationErrors {
    fn push(&mut self, diagnostic: Diagnostic) {
        if self.exhausted {
            return;
        }
        if self.diagnostics.len() < MAX_MIR_DIAGNOSTICS.saturating_sub(1) {
            self.diagnostics.push(diagnostic);
            return;
        }
        self.diagnostics.push(Diagnostic::error(
            "ZRYNA-N1202",
            None,
            format!(
                "native MIR verification reached its diagnostic limit of {MAX_MIR_DIAGNOSTICS}"
            ),
            "fix the retained diagnostics before verifying the module again",
        ));
        self.exhausted = true;
    }

    fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    const fn exhausted(&self) -> bool {
        self.exhausted
    }

    fn finish(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(id: u32, ty: MirType, operation: raw::Operation) -> raw::ValueDefinition {
        raw::ValueDefinition::new(raw::ValueId::new(id), ty, operation)
    }

    fn function(symbol: &str) -> raw::Function {
        raw::Function::new(
            symbol.to_owned(),
            raw::CallingConvention::ZRYNA_INTERNAL_I32_V1,
            raw::Signature::new(Vec::new(), MirType::I32),
            vec![value(0, MirType::I32, raw::Operation::I32Literal { value: 1 })],
            raw::ValueId::new(0),
        )
    }

    fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
        diagnostics.iter().map(Diagnostic::code).collect()
    }

    #[test]
    fn verifier_exposes_only_verified_views() {
        let raw = raw::Function::new(
            "add".to_owned(),
            raw::CallingConvention::ZRYNA_INTERNAL_I32_V1,
            raw::Signature::new(vec![MirType::I32, MirType::I32], MirType::I32),
            vec![
                value(0, MirType::I32, raw::Operation::Parameter { index: 0 }),
                value(1, MirType::I32, raw::Operation::Parameter { index: 1 }),
                value(
                    2,
                    MirType::I32,
                    raw::Operation::I32Add { lhs: raw::ValueId::new(0), rhs: raw::ValueId::new(1) },
                ),
            ],
            raw::ValueId::new(2),
        );
        let verified = verify(raw::Module::new(vec![raw])).expect("valid raw MIR must verify");
        let functions = verified.functions().collect::<Vec<_>>();
        assert_eq!(functions.len(), 1);
        let function = functions[0];
        assert_eq!(function.symbol(), "zryna_v1_e_add");
        assert_eq!(
            function.calling_convention(),
            VerifiedCallingConvention::ScalarAbiV1LinuxX8664SystemV
        );
        assert_eq!(function.parameter_types(), &[MirType::I32, MirType::I32]);
        assert_eq!(function.result_type(), MirType::I32);
        assert_eq!(function.result().index(), 2);
        assert_eq!(function.values().len(), 3);
        assert_eq!(
            function.value(ValueId(2)).map(VerifiedValue::operation),
            Some(OperationView::I32Add { lhs: ValueId(0), rhs: ValueId(1) })
        );
        assert!(verify(raw::Module::new(Vec::new())).is_ok());
    }

    #[test]
    fn scalar_abi_seal_invariant_has_a_distinct_code() {
        let raw = raw::Module::new(vec![function("default")]);
        assert_eq!(
            codes(&verify_scalar_abi(&raw).expect_err("reserved ABI name must fail")),
            ["ZRYNA-N1013"]
        );
    }

    #[test]
    fn accepts_shared_predecessors_and_dead_values() {
        let raw = raw::Function::new(
            "shared".to_owned(),
            raw::CallingConvention::ZRYNA_INTERNAL_I32_V1,
            raw::Signature::new(Vec::new(), MirType::I32),
            vec![
                value(0, MirType::I32, raw::Operation::I32Literal { value: 1 }),
                value(
                    1,
                    MirType::I32,
                    raw::Operation::I32Add { lhs: raw::ValueId::new(0), rhs: raw::ValueId::new(0) },
                ),
                value(
                    2,
                    MirType::I32,
                    raw::Operation::I32Add { lhs: raw::ValueId::new(0), rhs: raw::ValueId::new(0) },
                ),
            ],
            raw::ValueId::new(2),
        );
        assert!(verify(raw::Module::new(vec![raw])).is_ok());
    }

    #[test]
    fn rejects_unsafe_duplicate_and_portably_colliding_symbols() {
        for symbol in ["", "1value", "value-name", "$value", "é", "default", "a\n"] {
            let diagnostics = verify(raw::Module::new(vec![function(symbol)]))
                .expect_err("unsafe symbol must fail");
            assert_eq!(codes(&diagnostics), vec!["ZRYNA-N1002"], "symbol: {symbol:?}");
        }
        let exact = "a".repeat(MAX_MIR_SYMBOL_BYTES);
        assert!(verify(raw::Module::new(vec![function(&exact)])).is_ok());
        let too_long = "a".repeat(MAX_MIR_SYMBOL_BYTES + 1);
        assert_eq!(
            codes(
                &verify(raw::Module::new(vec![function(&too_long)])).expect_err("limit must fail")
            ),
            vec!["ZRYNA-N1201"]
        );
        assert!(
            codes(
                &verify(raw::Module::new(vec![function("same"), function("same")]))
                    .expect_err("duplicate symbol must fail")
            )
            .contains(&"ZRYNA-N1003")
        );
        assert!(
            codes(
                &verify(raw::Module::new(vec![function("valueName"), function("valuename")]))
                    .expect_err("portable collision must fail")
            )
            .contains(&"ZRYNA-N1003")
        );
    }

    #[test]
    fn rejects_unsupported_conventions_and_signature_types() {
        let mut wrong_convention = function("wrongConvention");
        wrong_convention.convention = raw::CallingConvention::from_code(u16::MAX);
        assert!(
            codes(
                &verify(raw::Module::new(vec![wrong_convention]))
                    .expect_err("unknown convention must fail")
            )
            .contains(&"ZRYNA-N1004")
        );
        for ty in [MirType::Bool, MirType::Unit] {
            let mut wrong_signature = function("wrongSignature");
            wrong_signature.signature = raw::Signature::new(vec![ty], ty);
            let diagnostics = verify(raw::Module::new(vec![wrong_signature]))
                .expect_err("reserved signature type must fail");
            assert!(codes(&diagnostics).contains(&"ZRYNA-N1005"));
        }
    }

    #[test]
    fn rejects_duplicate_gapped_and_reordered_value_definitions() {
        for ids in [[0, 0], [0, 2], [1, 0]] {
            let mut malformed = function("definitions");
            malformed.values = ids
                .into_iter()
                .map(|id| value(id, MirType::I32, raw::Operation::I32Literal { value: 1 }))
                .collect();
            malformed.result = raw::ValueId::new(0);
            assert!(
                codes(
                    &verify(raw::Module::new(vec![malformed]))
                        .expect_err("noncanonical definitions must fail")
                )
                .contains(&"ZRYNA-N1006")
            );
        }
    }

    #[test]
    fn rejects_missing_self_forward_and_cyclic_operands() {
        for (symbol, lhs) in [("missing", u32::MAX), ("selfEdge", 1), ("forward", 2)] {
            let raw = raw::Function::new(
                symbol.to_owned(),
                raw::CallingConvention::ZRYNA_INTERNAL_I32_V1,
                raw::Signature::new(Vec::new(), MirType::I32),
                vec![
                    value(0, MirType::I32, raw::Operation::I32Literal { value: 1 }),
                    value(
                        1,
                        MirType::I32,
                        raw::Operation::I32Add {
                            lhs: raw::ValueId::new(lhs),
                            rhs: raw::ValueId::new(0),
                        },
                    ),
                    value(2, MirType::I32, raw::Operation::I32Literal { value: 2 }),
                ],
                raw::ValueId::new(1),
            );
            let diagnostics = verify(raw::Module::new(vec![raw]))
                .expect_err("missing or non-predecessor operand must fail");
            if lhs == u32::MAX {
                assert!(codes(&diagnostics).contains(&"ZRYNA-N1007"));
            } else {
                assert!(codes(&diagnostics).contains(&"ZRYNA-N1008"));
            }
        }

        let cycle = raw::Function::new(
            "cycle".to_owned(),
            raw::CallingConvention::ZRYNA_INTERNAL_I32_V1,
            raw::Signature::new(Vec::new(), MirType::I32),
            vec![
                value(
                    0,
                    MirType::I32,
                    raw::Operation::I32Add { lhs: raw::ValueId::new(1), rhs: raw::ValueId::new(1) },
                ),
                value(
                    1,
                    MirType::I32,
                    raw::Operation::I32Add { lhs: raw::ValueId::new(0), rhs: raw::ValueId::new(0) },
                ),
            ],
            raw::ValueId::new(1),
        );
        assert!(
            codes(&verify(raw::Module::new(vec![cycle])).expect_err("cycle must fail"))
                .contains(&"ZRYNA-N1009")
        );
    }

    #[test]
    fn rejects_invalid_parameters_and_operation_types() {
        let mut bad_parameter = function("badParameter");
        bad_parameter.signature = raw::Signature::new(vec![MirType::I32], MirType::I32);
        bad_parameter.values[0] =
            value(0, MirType::Bool, raw::Operation::Parameter { index: u32::MAX });
        assert!(
            codes(
                &verify(raw::Module::new(vec![bad_parameter]))
                    .expect_err("bad parameter must fail")
            )
            .contains(&"ZRYNA-N1010")
        );

        let mut bad_literal = function("badLiteral");
        bad_literal.values[0].ty = MirType::Bool;
        assert!(
            codes(
                &verify(raw::Module::new(vec![bad_literal]))
                    .expect_err("mistyped literal must fail")
            )
            .contains(&"ZRYNA-N1011")
        );

        let mut bad_add = function("badAdd");
        bad_add.values.push(value(
            1,
            MirType::Bool,
            raw::Operation::I32Add { lhs: raw::ValueId::new(0), rhs: raw::ValueId::new(0) },
        ));
        bad_add.result = raw::ValueId::new(1);
        assert!(
            codes(&verify(raw::Module::new(vec![bad_add])).expect_err("mistyped add must fail"))
                .contains(&"ZRYNA-N1011")
        );
    }

    #[test]
    fn rejects_missing_and_mistyped_results() {
        let mut missing = function("missingResult");
        missing.result = raw::ValueId::new(u32::MAX);
        assert!(
            codes(&verify(raw::Module::new(vec![missing])).expect_err("missing result must fail"))
                .contains(&"ZRYNA-N1007")
        );

        let mut wrong_type = function("wrongResult");
        wrong_type.signature = raw::Signature::new(Vec::new(), MirType::Bool);
        let diagnostics =
            verify(raw::Module::new(vec![wrong_type])).expect_err("wrong result type must fail");
        assert!(codes(&diagnostics).contains(&"ZRYNA-N1012"));
    }

    #[test]
    fn resource_preflight_rejects_each_first_extra() {
        let mut too_many_parameters = function("parameters");
        too_many_parameters.signature = raw::Signature::new(
            vec![MirType::I32; MAX_MIR_PARAMETERS_PER_FUNCTION + 1],
            MirType::I32,
        );
        assert_eq!(
            codes(
                &verify(raw::Module::new(vec![too_many_parameters]))
                    .expect_err("parameter limit +1 must fail")
            ),
            vec!["ZRYNA-N1201"]
        );

        let mut too_many_values = function("values");
        too_many_values.values = (0..=MAX_MIR_VALUES_PER_FUNCTION)
            .map(|index| {
                value(
                    u32::try_from(index).expect("bounded fixture"),
                    MirType::I32,
                    raw::Operation::I32Literal { value: 1 },
                )
            })
            .collect();
        assert_eq!(
            codes(
                &verify(raw::Module::new(vec![too_many_values]))
                    .expect_err("value limit +1 must fail")
            ),
            vec!["ZRYNA-N1201"]
        );

        assert_eq!(
            codes(
                &verify(raw::Module::new(vec![function("f"); MAX_MIR_FUNCTIONS + 1]))
                    .expect_err("function limit +1 must fail")
            ),
            vec!["ZRYNA-N1201"]
        );
    }

    #[test]
    fn aggregate_limits_accept_exact_and_reject_first_extra() {
        let mut parameter_function = function("parameters");
        parameter_function.signature =
            raw::Signature::new(vec![MirType::I32; MAX_MIR_PARAMETERS_PER_FUNCTION], MirType::I32);
        let exact_parameters = raw::Module::new(vec![
            parameter_function.clone();
            MAX_MIR_PARAMETERS_PER_MODULE
                / MAX_MIR_PARAMETERS_PER_FUNCTION
        ]);
        let mut errors = VerificationErrors::default();
        verify_resource_limits(&exact_parameters, &mut errors);
        assert!(errors.is_empty());
        let mut extra_parameters = exact_parameters;
        extra_parameters.functions.push(parameter_function);
        let mut errors = VerificationErrors::default();
        verify_resource_limits(&extra_parameters, &mut errors);
        assert_eq!(codes(&errors.finish()), vec!["ZRYNA-N1201"]);

        let values = (0..MAX_MIR_VALUES_PER_FUNCTION)
            .map(|index| {
                value(
                    u32::try_from(index).expect("bounded fixture"),
                    MirType::I32,
                    raw::Operation::I32Literal { value: 1 },
                )
            })
            .collect::<Vec<_>>();
        let mut value_function = function("values");
        value_function.values = values;
        let exact_values = raw::Module::new(vec![
            value_function.clone();
            MAX_MIR_VALUES_PER_MODULE
                / MAX_MIR_VALUES_PER_FUNCTION
        ]);
        let mut errors = VerificationErrors::default();
        verify_resource_limits(&exact_values, &mut errors);
        assert!(errors.is_empty());
        let mut extra_values = exact_values;
        extra_values.functions.push(value_function);
        let mut errors = VerificationErrors::default();
        verify_resource_limits(&extra_values, &mut errors);
        assert_eq!(codes(&errors.finish()), vec!["ZRYNA-N1201"]);
    }

    #[test]
    fn diagnostic_budget_is_bounded_and_terminal() {
        let values = (0..300_u32)
            .map(|index| value(index, MirType::Bool, raw::Operation::I32Literal { value: 1 }))
            .collect();
        let malformed = raw::Function::new(
            "manyErrors".to_owned(),
            raw::CallingConvention::ZRYNA_INTERNAL_I32_V1,
            raw::Signature::new(Vec::new(), MirType::I32),
            values,
            raw::ValueId::new(299),
        );
        let diagnostics = verify(raw::Module::new(vec![malformed]))
            .expect_err("diagnostic limit fixture must fail");
        assert_eq!(diagnostics.len(), MAX_MIR_DIAGNOSTICS);
        assert_eq!(diagnostics.last().map(Diagnostic::code), Some("ZRYNA-N1202"));
    }

    #[test]
    fn maximum_value_chain_verifies_iteratively() {
        let mut values = vec![value(0, MirType::I32, raw::Operation::I32Literal { value: 1 })];
        for index in 1..MAX_MIR_VALUES_PER_FUNCTION {
            let id = u32::try_from(index).expect("bounded fixture");
            let predecessor = raw::ValueId::new(id - 1);
            values.push(value(
                id,
                MirType::I32,
                raw::Operation::I32Add { lhs: predecessor, rhs: predecessor },
            ));
        }
        let function = raw::Function::new(
            "maximum".to_owned(),
            raw::CallingConvention::ZRYNA_INTERNAL_I32_V1,
            raw::Signature::new(Vec::new(), MirType::I32),
            values,
            raw::ValueId::new(
                u32::try_from(MAX_MIR_VALUES_PER_FUNCTION - 1).expect("bounded fixture"),
            ),
        );
        assert!(verify(raw::Module::new(vec![function])).is_ok());
    }
}
