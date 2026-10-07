//! Verified target-neutral Zryna intermediate representation.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use zryna_abi::{AbiViolationKind, raw as raw_abi, verify_v1};
use zryna_diagnostics::Diagnostic;
use zryna_source::{SourceMap, Span};

/// Versioned structured control-flow IR for the planned M2 profile.
pub mod control_flow_v1;
/// Separately verified data, ownership, and cleanup IR for the planned M3 profile.
pub mod data_ownership_v1;
/// Independent bounded closed IR checks; executable source sealing remains separate.
pub mod generic_v1;

pub use zryna_abi::{LogicalExportName, VerifiedScalarExport};

/// Maximum functions accepted in one Universal IR program.
pub const MAX_IR_FUNCTIONS: usize = 16_384;
/// Maximum parameters accepted in one Universal IR function.
pub const MAX_IR_PARAMETERS_PER_FUNCTION: usize = 256;
/// Maximum parameters accepted across one Universal IR program.
pub const MAX_IR_PARAMETERS_PER_PROGRAM: usize = 262_144;
/// Maximum expressions accepted in one Universal IR function.
pub const MAX_IR_EXPRESSIONS_PER_FUNCTION: usize = 16_384;
/// Maximum expressions accepted across one Universal IR program.
pub const MAX_IR_EXPRESSIONS_PER_PROGRAM: usize = 262_144;
/// Maximum expression-tree depth accepted by the current universal profile.
pub const MAX_IR_EXPRESSION_DEPTH: u32 = 128;
/// Maximum bytes accepted in one logical export name.
pub const MAX_IR_EXPORT_NAME_BYTES: usize = zryna_abi::MAX_LOGICAL_EXPORT_NAME_BYTES;
/// Maximum retained verifier diagnostics, including the terminal budget diagnostic.
pub const MAX_IR_DIAGNOSTICS: usize = 256;

mod scalar_type;
pub use scalar_type::Type;

/// Expression identifier within one function.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExprId(pub u32);

/// Typed expression before Universal IR verification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Expr {
    /// Claimed result type.
    pub ty: Type,
    /// Source range.
    pub span: Span,
    /// Operation with fully selected semantics.
    pub kind: ExprKind,
}

/// Exact target-neutral expression operations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExprKind {
    /// Function parameter by zero-based index.
    Parameter(u32),
    /// Boolean literal reserved for semantic lowering before a bool-capable universal profile.
    BoolLiteral(bool),
    /// Signed wrapping 32-bit integer addition.
    I32Add {
        /// Left operand.
        lhs: ExprId,
        /// Right operand.
        rhs: ExprId,
    },
    /// Signed 32-bit integer literal.
    I32Literal(i32),
}

/// Typed function before Universal IR verification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Function {
    /// Claimed logical export name.
    pub name: String,
    /// Parameter types.
    pub parameters: Vec<Type>,
    /// Return type.
    pub return_type: Type,
    /// Function expression arena in claimed canonical postorder.
    pub expressions: Vec<Expr>,
    /// Returned root expression.
    pub body: ExprId,
}

/// Zryna compilation unit before verification.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Program {
    /// Functions in deterministic source order.
    pub functions: Vec<Function>,
}

/// Backend-safe scalar intersection carried by a [`VerifiedProgram`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UniversalProfile {
    /// Initial profile containing only wrapping signed 32-bit integer values.
    I32V1,
}

/// Program proven to satisfy every invariant in the current universal backend profile.
///
/// Construction is reserved to [`verify`]:
///
/// ```compile_fail
/// let raw = zryna_ir::Program::default();
/// let _ = zryna_ir::VerifiedProgram { program: raw, abi: todo!() };
/// ```
#[derive(Clone, Debug)]
pub struct VerifiedProgram {
    program: Program,
    abi: zryna_abi::VerifiedScalarAbiModule,
}

impl VerifiedProgram {
    /// Returns the exact backend-safe profile carried by this program.
    #[must_use]
    pub const fn profile(&self) -> UniversalProfile {
        UniversalProfile::I32V1
    }

    /// Iterates immutable backend-safe function views in deterministic source order.
    #[must_use]
    pub fn functions(&self) -> impl ExactSizeIterator<Item = VerifiedFunction<'_>> {
        self.program
            .functions
            .iter()
            .zip(self.abi.exports())
            .map(|(function, abi_export)| VerifiedFunction { function, abi_export })
    }

    /// Returns the sealed scalar ABI authority embedded by verification.
    #[must_use]
    pub const fn scalar_abi(&self) -> &zryna_abi::VerifiedScalarAbiModule {
        &self.abi
    }

    /// Validates one typed invocation against the embedded scalar ABI authority.
    ///
    /// # Errors
    ///
    /// Rejects an unknown export, wrong arity, or mismatched typed argument.
    pub fn prepare_invocation(
        &self,
        invocation: zryna_abi::Invocation,
    ) -> Result<zryna_abi::VerifiedInvocation<'_>, zryna_abi::InvocationError> {
        self.abi.prepare_invocation(invocation)
    }
}

/// Immutable view of one function inside a [`VerifiedProgram`].
#[derive(Clone, Copy, Debug)]
pub struct VerifiedFunction<'program> {
    function: &'program Function,
    abi_export: VerifiedScalarExport<'program>,
}

impl<'program> VerifiedFunction<'program> {
    /// Returns the validated logical export name.
    #[must_use]
    pub const fn export_name(self) -> &'program LogicalExportName {
        self.abi_export.logical_name()
    }

    /// Returns the complete verified scalar ABI mapping for this export.
    #[must_use]
    pub const fn abi_export(self) -> VerifiedScalarExport<'program> {
        self.abi_export
    }

    /// Returns the current-profile parameter types.
    #[must_use]
    pub fn parameters(self) -> &'program [Type] {
        &self.function.parameters
    }

    /// Returns the current-profile result type.
    #[must_use]
    pub const fn return_type(self) -> Type {
        self.function.return_type
    }

    /// Returns the verified canonical expression arena.
    #[must_use]
    pub fn expressions(self) -> &'program [Expr] {
        &self.function.expressions
    }

    /// Returns the verified root expression identifier.
    #[must_use]
    pub const fn body(self) -> ExprId {
        self.function.body
    }
}

/// Verifies all invariants required by every backend in the current universal profile.
///
/// The verifier is iterative and bounded. It accepts only `i32`, exact backend-safe export names,
/// source-map-authoritative spans, and one single-owner expression tree in canonical postorder.
///
/// # Errors
///
/// Returns deterministic, bounded diagnostics when any trust invariant is unproven.
pub fn verify(program: Program, sources: &SourceMap) -> Result<VerifiedProgram, Vec<Diagnostic>> {
    let mut errors = VerificationErrors::default();
    verify_resource_limits(&program, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }

    let abi = verify_abi(&program, &mut errors);
    for (function_index, function) in program.functions.iter().enumerate() {
        if errors.exhausted() {
            break;
        }
        verify_function(function_index, function, sources, &mut errors);
    }
    if !errors.is_empty() {
        return Err(errors.finish());
    }

    let Some(abi) = abi else {
        return Err(vec![Diagnostic::error(
            "ZRYNA-I1202",
            None,
            "IR verifier could not construct its bounded scalar ABI table",
            "report this compiler invariant failure with the smallest reproducible source",
        )]);
    };
    Ok(VerifiedProgram { program, abi })
}

fn verify_resource_limits(program: &Program, errors: &mut VerificationErrors) {
    if program.functions.len() > MAX_IR_FUNCTIONS {
        errors.push(limit_error("function count", MAX_IR_FUNCTIONS));
        return;
    }

    let mut parameters = 0_usize;
    let mut expressions = 0_usize;
    for (function_index, function) in program.functions.iter().enumerate() {
        if function.parameters.len() > MAX_IR_PARAMETERS_PER_FUNCTION {
            errors.push(function_limit_error(
                function_index,
                "parameters",
                MAX_IR_PARAMETERS_PER_FUNCTION,
            ));
        }
        if function.expressions.len() > MAX_IR_EXPRESSIONS_PER_FUNCTION {
            errors.push(function_limit_error(
                function_index,
                "expressions",
                MAX_IR_EXPRESSIONS_PER_FUNCTION,
            ));
        }
        parameters = if let Some(total) = parameters.checked_add(function.parameters.len()) {
            total
        } else {
            errors.push(limit_error("aggregate parameter count", MAX_IR_PARAMETERS_PER_PROGRAM));
            return;
        };
        expressions = if let Some(total) = expressions.checked_add(function.expressions.len()) {
            total
        } else {
            errors.push(limit_error("aggregate expression count", MAX_IR_EXPRESSIONS_PER_PROGRAM));
            return;
        };
    }
    if parameters > MAX_IR_PARAMETERS_PER_PROGRAM {
        errors.push(limit_error("aggregate parameter count", MAX_IR_PARAMETERS_PER_PROGRAM));
    }
    if expressions > MAX_IR_EXPRESSIONS_PER_PROGRAM {
        errors.push(limit_error("aggregate expression count", MAX_IR_EXPRESSIONS_PER_PROGRAM));
    }
}

fn limit_error(label: &str, limit: usize) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-I1201",
        None,
        format!("Universal IR {label} exceeds its limit of {limit}"),
        "reduce the program before Universal IR verification",
    )
}

fn function_limit_error(function_index: usize, label: &str, limit: usize) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-I1201",
        None,
        format!("function #{function_index} has too many {label}; the limit is {limit}"),
        "reduce the function before Universal IR verification",
    )
}

fn verify_abi(
    program: &Program,
    errors: &mut VerificationErrors,
) -> Option<zryna_abi::VerifiedScalarAbiModule> {
    let exports = program
        .functions
        .iter()
        .map(|function| {
            raw_abi::Export::new(
                function.name.clone(),
                raw_abi::Signature::new(
                    function.parameters.iter().copied().map(raw_abi_type).collect(),
                    raw_abi_type(function.return_type),
                ),
            )
        })
        .collect();
    match verify_v1(raw_abi::Module::new(exports)) {
        Ok(abi) => Some(abi),
        Err(violations) => {
            for violation in violations {
                let function_index = violation.export_index().unwrap_or(0);
                match violation.kind() {
                    AbiViolationKind::InvalidLogicalName => errors.push(Diagnostic::error(
                        "ZRYNA-I1009",
                        None,
                        format!("function #{function_index} has an invalid logical export name"),
                        "use 1 to 128 ASCII bytes matching [A-Za-z_][A-Za-z0-9_]* and avoid reserved bindings",
                    )),
                    AbiViolationKind::DuplicateLogicalName { first_index } => {
                        errors.push(Diagnostic::error(
                            "ZRYNA-I1010",
                            None,
                            format!(
                                "function #{function_index} duplicates the logical export of function #{first_index}"
                            ),
                            "give every exported function one exact unique logical name",
                        ));
                    }
                    AbiViolationKind::PortableNameCollision { first_index } => {
                        errors.push(Diagnostic::error(
                            "ZRYNA-I1011",
                            None,
                            format!(
                                "function #{function_index} collides with function #{first_index} under the portable target-symbol identity"
                            ),
                            "use export names that remain unique when ASCII case is ignored",
                        ));
                    }
                    AbiViolationKind::UnsupportedScalarType => {}
                    AbiViolationKind::TooManyExports
                    | AbiViolationKind::TooManyParameters
                    | AbiViolationKind::TooManyParametersInModule => errors.push(Diagnostic::error(
                        "ZRYNA-I1201",
                        None,
                        "Universal IR scalar ABI claims exceed their resource limits",
                        "reduce the program before Universal IR verification",
                    )),
                    AbiViolationKind::ViolationBudgetExceeded => errors.push(Diagnostic::error(
                        "ZRYNA-I1202",
                        None,
                        "Universal IR scalar ABI diagnostics exceeded their limit",
                        "fix earlier scalar ABI diagnostics before compiling again",
                    )),
                }
            }
            None
        }
    }
}

const fn raw_abi_type(ty: Type) -> raw_abi::Type {
    match ty {
        Type::Unit => raw_abi::Type::Unit,
        Type::Bool => raw_abi::Type::Bool,
        Type::I32 => raw_abi::Type::I32,
    }
}

fn verify_function(
    function_index: usize,
    function: &Function,
    sources: &SourceMap,
    errors: &mut VerificationErrors,
) {
    for (parameter_index, ty) in function.parameters.iter().enumerate() {
        if !profile_supports(*ty) {
            errors.push(unsupported_type_error(function_index, "parameter", parameter_index));
        }
    }
    if !profile_supports(function.return_type) {
        errors.push(Diagnostic::error(
            "ZRYNA-I1006",
            None,
            format!("function #{function_index} has an unsupported result type"),
            "use only i32 until a wider universal backend profile is enabled",
        ));
    }

    let valid_spans = function
        .expressions
        .iter()
        .map(|expression| match sources.resolve(expression.span) {
            Ok(_) => true,
            Err(error) => {
                errors.push(Diagnostic::from_source_error(&error));
                false
            }
        })
        .collect::<Vec<_>>();

    let body_index =
        usize::try_from(function.body.0).ok().filter(|index| *index < function.expressions.len());
    let Some(body_index) = body_index else {
        errors.push(Diagnostic::error(
            "ZRYNA-I1001",
            None,
            format!("function #{function_index} has an invalid body expression"),
            "produce one body root that belongs to the same canonical expression arena",
        ));
        verify_expressions(function_index, function, &valid_spans, None, errors);
        return;
    };

    let body = &function.expressions[body_index];
    if valid_spans[body_index] && body.ty != function.return_type {
        errors.push(Diagnostic::error_at(
            "ZRYNA-I1002",
            body.span,
            format!("function #{function_index} returns the wrong IR type"),
            "make the body expression type equal the declared return type",
        ));
    }
    verify_expressions(function_index, function, &valid_spans, Some(body_index), errors);
}

fn unsupported_type_error(function_index: usize, label: &str, item_index: usize) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-I1006",
        None,
        format!("function #{function_index} {label} #{item_index} has an unsupported type"),
        "use only i32 until a wider universal backend profile is enabled",
    )
}

const fn profile_supports(ty: Type) -> bool {
    matches!(ty, Type::I32)
}

fn verify_expressions(
    function_index: usize,
    function: &Function,
    valid_spans: &[bool],
    body_index: Option<usize>,
    errors: &mut VerificationErrors,
) {
    let mut depths = Vec::<u32>::with_capacity(function.expressions.len());
    let mut owners = vec![0_u32; function.expressions.len()];
    let mut graph_valid = true;
    for (expression_index, expression) in function.expressions.iter().enumerate() {
        if errors.exhausted() {
            return;
        }
        if !profile_supports(expression.ty) {
            push_expression_error(
                errors,
                valid_spans[expression_index],
                expression.span,
                "ZRYNA-I1006",
                format!(
                    "function #{function_index} expression #{expression_index} has an unsupported type"
                ),
                "use only i32 until a wider universal backend profile is enabled",
            );
        }
        let (depth, expression_graph_valid) = verify_expression(
            function_index,
            function,
            expression_index,
            valid_spans[expression_index],
            &depths,
            &mut owners,
            errors,
        );
        graph_valid &= expression_graph_valid;
        if depth > MAX_IR_EXPRESSION_DEPTH {
            graph_valid = false;
            push_expression_error(
                errors,
                valid_spans[expression_index],
                expression.span,
                "ZRYNA-I1008",
                format!(
                    "function #{function_index} expression #{expression_index} exceeds depth {MAX_IR_EXPRESSION_DEPTH}"
                ),
                "split the expression before Universal IR verification",
            );
        }
        depths.push(depth);
    }

    let Some(body_index) = body_index else {
        return;
    };
    owners[body_index] = owners[body_index].saturating_add(1);
    if graph_valid
        && let Some((expression_index, owner_count)) =
            owners.iter().copied().enumerate().find(|(_, count)| *count != 1)
    {
        graph_valid = false;
        push_expression_error(
            errors,
            valid_spans[expression_index],
            function.expressions[expression_index].span,
            "ZRYNA-I1008",
            format!(
                "function #{function_index} expression #{expression_index} has {owner_count} owners; expected exactly one"
            ),
            "return one expression tree with no shared or orphan arena entries",
        );
    }
    if graph_valid {
        verify_canonical_postorder(function_index, function, body_index, valid_spans, errors);
    }
}

fn verify_expression(
    function_index: usize,
    function: &Function,
    expression_index: usize,
    valid_span: bool,
    depths: &[u32],
    owners: &mut [u32],
    errors: &mut VerificationErrors,
) -> (u32, bool) {
    let expression = &function.expressions[expression_index];
    match &expression.kind {
        ExprKind::Parameter(index) => {
            let parameter =
                usize::try_from(*index).ok().and_then(|value| function.parameters.get(value));
            if parameter != Some(&expression.ty) {
                push_expression_error(
                    errors,
                    valid_span,
                    expression.span,
                    "ZRYNA-I1003",
                    format!(
                        "function #{function_index} expression #{expression_index} references an invalid parameter"
                    ),
                    "use an existing parameter index with the exact verified type",
                );
            }
            (1, true)
        }
        ExprKind::BoolLiteral(_) => {
            if expression.ty != Type::Bool {
                push_expression_error(
                    errors,
                    valid_span,
                    expression.span,
                    "ZRYNA-I1005",
                    format!(
                        "function #{function_index} expression #{expression_index} is a mistyped bool literal"
                    ),
                    "assign Type::Bool to every bool literal",
                );
            }
            (1, true)
        }
        ExprKind::I32Literal(_) => {
            if expression.ty != Type::I32 {
                push_expression_error(
                    errors,
                    valid_span,
                    expression.span,
                    "ZRYNA-I1005",
                    format!(
                        "function #{function_index} expression #{expression_index} is a mistyped i32 literal"
                    ),
                    "assign Type::I32 to every i32 literal",
                );
            }
            (1, true)
        }
        ExprKind::I32Add { lhs, rhs } => {
            let left = predecessor(*lhs, expression_index);
            let right = predecessor(*rhs, expression_index);
            let predecessors = match (left, right) {
                (Some(left), Some(right)) if lhs != rhs => Some((left, right)),
                _ => None,
            };
            let Some((left, right)) = predecessors else {
                push_expression_error(
                    errors,
                    valid_span,
                    expression.span,
                    "ZRYNA-I1007",
                    format!(
                        "function #{function_index} expression #{expression_index} has a missing, shared, self, or forward operand"
                    ),
                    "reference two distinct earlier expressions in canonical postorder",
                );
                return (1, false);
            };
            owners[left] = owners[left].saturating_add(1);
            owners[right] = owners[right].saturating_add(1);
            let operands_are_i32 = function.expressions[left].ty == Type::I32
                && function.expressions[right].ty == Type::I32;
            if expression.ty != Type::I32 || !operands_are_i32 {
                push_expression_error(
                    errors,
                    valid_span,
                    expression.span,
                    "ZRYNA-I1004",
                    format!(
                        "function #{function_index} expression #{expression_index} is an invalid i32 addition"
                    ),
                    "i32 addition requires two earlier i32 operands and an i32 result",
                );
            }
            (depths[left].max(depths[right]).saturating_add(1), true)
        }
    }
}

fn predecessor(id: ExprId, current: usize) -> Option<usize> {
    usize::try_from(id.0).ok().filter(|index| *index < current)
}

fn verify_canonical_postorder(
    function_index: usize,
    function: &Function,
    body_index: usize,
    valid_spans: &[bool],
    errors: &mut VerificationErrors,
) {
    let mut emitted = vec![false; function.expressions.len()];
    let mut expected = 0_usize;
    let mut stack = vec![(body_index, false)];
    while let Some((index, exiting)) = stack.pop() {
        if exiting {
            if emitted[index] {
                continue;
            }
            if index != expected {
                push_expression_error(
                    errors,
                    valid_spans[index],
                    function.expressions[index].span,
                    "ZRYNA-I1008",
                    format!(
                        "function #{function_index} expression arena is not canonical postorder: expected #{expected}, found #{index}"
                    ),
                    "emit the one expression tree left-to-right in exact postorder",
                );
                return;
            }
            emitted[index] = true;
            expected = expected.saturating_add(1);
            continue;
        }
        if emitted[index] {
            continue;
        }
        stack.push((index, true));
        if let ExprKind::I32Add { lhs, rhs } = function.expressions[index].kind {
            let Some(left) = predecessor(lhs, index) else {
                return;
            };
            let Some(right) = predecessor(rhs, index) else {
                return;
            };
            stack.push((right, false));
            stack.push((left, false));
        }
    }
}

fn push_expression_error(
    errors: &mut VerificationErrors,
    valid_span: bool,
    span: Span,
    code: &'static str,
    message: String,
    guidance: &'static str,
) {
    let diagnostic = if valid_span {
        Diagnostic::error_at(code, span, message, guidance)
    } else {
        Diagnostic::error(code, None, message, guidance)
    };
    errors.push(diagnostic);
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
        if self.diagnostics.len() < MAX_IR_DIAGNOSTICS.saturating_sub(1) {
            self.diagnostics.push(diagnostic);
            return;
        }
        self.diagnostics.push(Diagnostic::error(
            "ZRYNA-I1202",
            None,
            format!(
                "Universal IR verification reached its diagnostic limit of {MAX_IR_DIAGNOSTICS}"
            ),
            "fix the retained diagnostics before verifying the program again",
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
    use zryna_diagnostics::render_text;
    use zryna_source::{NormalizedSourcePath, SourceFileInput};

    fn sources() -> SourceMap {
        SourceMap::build(vec![SourceFileInput {
            path: "src/main.zry".to_owned(),
            text: "x".to_owned(),
        }])
        .expect("fixture source map must be valid")
    }

    fn span(sources: &SourceMap) -> Span {
        let path = NormalizedSourcePath::new("src/main.zry").expect("fixture path must be valid");
        let file = sources.file_id(&path).expect("fixture file must exist");
        sources.span(file, 0, 1).expect("fixture span must be valid")
    }

    fn literal_function(name: &str, span: Span) -> Function {
        Function {
            name: name.to_owned(),
            parameters: Vec::new(),
            return_type: Type::I32,
            expressions: vec![Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(1) }],
            body: ExprId(0),
        }
    }

    fn program(sources: &SourceMap) -> Program {
        Program { functions: vec![literal_function("value", span(sources))] }
    }

    fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
        diagnostics.iter().map(Diagnostic::code).collect()
    }

    #[test]
    fn verified_program_exposes_only_sealed_function_views() {
        let sources = sources();
        let verified = verify(program(&sources), &sources).expect("fixture IR must verify");
        assert_eq!(verified.profile(), UniversalProfile::I32V1);
        let functions = verified.functions().collect::<Vec<_>>();
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].export_name().as_str(), "value");
        assert_eq!(functions[0].abi_export().javascript_name().as_str(), "value");
        assert_eq!(functions[0].abi_export().webassembly_name().as_str(), "value");
        assert_eq!(
            functions[0].abi_export().native_linux_x86_64_symbol().as_str(),
            "zryna_v1_e_value"
        );
        assert_eq!(functions[0].return_type(), Type::I32);
        assert_eq!(functions[0].body(), ExprId(0));
    }

    #[test]
    fn verified_program_rejects_a_span_from_another_source_map() {
        let first = sources();
        let second = sources();
        assert!(verify(program(&first), &first).is_ok());
        let diagnostics = verify(program(&first), &second)
            .expect_err("cross-map spans must not enter VerifiedProgram");
        assert_eq!(codes(&diagnostics), vec!["ZRYNA-S1006"]);
    }

    #[test]
    fn invalid_span_diagnostics_remain_renderable_when_types_are_also_wrong() {
        let first = sources();
        let second = sources();
        let mut invalid = program(&first);
        invalid.functions[0].return_type = Type::Bool;
        invalid.functions[0].expressions[0].ty = Type::Bool;

        let diagnostics =
            verify(invalid, &second).expect_err("wrong-map spans and types must fail");
        assert!(codes(&diagnostics).contains(&"ZRYNA-S1006"));
        assert!(codes(&diagnostics).contains(&"ZRYNA-I1006"));
        assert!(render_text(&diagnostics, &second).is_ok());
    }

    #[test]
    fn rejects_missing_self_forward_shared_and_cyclic_expression_edges() {
        let sources = sources();
        let span = span(&sources);
        let base = Function {
            name: "add".to_owned(),
            parameters: Vec::new(),
            return_type: Type::I32,
            expressions: vec![
                Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(1) },
                Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(2) },
                Expr {
                    ty: Type::I32,
                    span,
                    kind: ExprKind::I32Add { lhs: ExprId(0), rhs: ExprId(1) },
                },
            ],
            body: ExprId(2),
        };
        let mut mutations = Vec::new();
        let mut missing = base.clone();
        missing.expressions[2].kind = ExprKind::I32Add { lhs: ExprId(0), rhs: ExprId(u32::MAX) };
        mutations.push(missing);
        let mut self_edge = base.clone();
        self_edge.expressions[2].kind = ExprKind::I32Add { lhs: ExprId(0), rhs: ExprId(2) };
        mutations.push(self_edge);
        let mut shared = base.clone();
        shared.expressions[2].kind = ExprKind::I32Add { lhs: ExprId(0), rhs: ExprId(0) };
        mutations.push(shared);
        let mut forward_cycle = base;
        forward_cycle.expressions[0].kind = ExprKind::I32Add { lhs: ExprId(1), rhs: ExprId(2) };
        mutations.push(forward_cycle);

        for mutation in mutations {
            let diagnostics = verify(Program { functions: vec![mutation] }, &sources)
                .expect_err("non-predecessor graphs must fail");
            assert!(codes(&diagnostics).contains(&"ZRYNA-I1007"));
        }
    }

    #[test]
    fn rejects_noncanonical_shared_and_orphan_arenas() {
        let sources = sources();
        let span = span(&sources);
        let function = Function {
            name: "wrongOrder".to_owned(),
            parameters: Vec::new(),
            return_type: Type::I32,
            expressions: vec![
                Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(1) },
                Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(2) },
                Expr {
                    ty: Type::I32,
                    span,
                    kind: ExprKind::I32Add { lhs: ExprId(1), rhs: ExprId(0) },
                },
            ],
            body: ExprId(2),
        };
        let diagnostics = verify(Program { functions: vec![function] }, &sources)
            .expect_err("noncanonical postorder must fail");
        assert!(codes(&diagnostics).contains(&"ZRYNA-I1008"));

        let mut orphan = program(&sources);
        orphan.functions[0].expressions.push(Expr {
            ty: Type::I32,
            span,
            kind: ExprKind::I32Literal(2),
        });
        let diagnostics = verify(orphan, &sources).expect_err("orphan values must fail");
        assert!(codes(&diagnostics).contains(&"ZRYNA-I1008"));
    }

    #[test]
    fn expression_depth_accepts_128_and_rejects_129_without_recursion() {
        let sources = sources();
        assert!(verify(deep_program(&sources, MAX_IR_EXPRESSION_DEPTH), &sources).is_ok());
        let diagnostics = verify(deep_program(&sources, MAX_IR_EXPRESSION_DEPTH + 1), &sources)
            .expect_err("depth +1 must fail");
        assert!(codes(&diagnostics).contains(&"ZRYNA-I1008"));
    }

    fn deep_program(sources: &SourceMap, depth: u32) -> Program {
        let span = span(sources);
        let mut expressions = vec![Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(1) }];
        let mut root = ExprId(0);
        for _ in 1..depth {
            let leaf = ExprId(u32::try_from(expressions.len()).expect("bounded fixture"));
            expressions.push(Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(1) });
            let parent = ExprId(u32::try_from(expressions.len()).expect("bounded fixture"));
            expressions.push(Expr {
                ty: Type::I32,
                span,
                kind: ExprKind::I32Add { lhs: root, rhs: leaf },
            });
            root = parent;
        }
        Program {
            functions: vec![Function {
                name: "deepValue".to_owned(),
                parameters: Vec::new(),
                return_type: Type::I32,
                expressions,
                body: root,
            }],
        }
    }

    #[test]
    fn rejects_invalid_duplicate_and_portably_colliding_exports() {
        let sources = sources();
        for name in ["", "1value", "value-name", "$value", "é", "default", "then", "a\n"] {
            let diagnostics = verify(
                Program { functions: vec![literal_function(name, span(&sources))] },
                &sources,
            )
            .expect_err("unsafe exports must fail");
            assert_eq!(codes(&diagnostics), vec!["ZRYNA-I1009"]);
        }
        let exact = "a".repeat(MAX_IR_EXPORT_NAME_BYTES);
        assert!(
            verify(Program { functions: vec![literal_function(&exact, span(&sources))] }, &sources)
                .is_ok()
        );
        let too_long = "a".repeat(MAX_IR_EXPORT_NAME_BYTES + 1);
        assert_eq!(
            codes(
                &verify(
                    Program { functions: vec![literal_function(&too_long, span(&sources))] },
                    &sources
                )
                .expect_err("overlong export must fail")
            ),
            vec!["ZRYNA-I1009"]
        );

        let duplicate = Program {
            functions: vec![
                literal_function("same", span(&sources)),
                literal_function("same", span(&sources)),
            ],
        };
        assert!(
            codes(&verify(duplicate, &sources).expect_err("duplicates must fail"))
                .contains(&"ZRYNA-I1010")
        );
        let collision = Program {
            functions: vec![
                literal_function("valueName", span(&sources)),
                literal_function("valuename", span(&sources)),
            ],
        };
        assert!(
            codes(&verify(collision, &sources).expect_err("portable collision must fail"))
                .contains(&"ZRYNA-I1011")
        );
    }

    #[test]
    fn current_profile_rejects_bool_unit_and_invalid_parameter_types() {
        let sources = sources();
        let span = span(&sources);
        for ty in [Type::Bool, Type::Unit] {
            let function = Function {
                name: "reservedType".to_owned(),
                parameters: vec![ty],
                return_type: ty,
                expressions: vec![Expr { ty, span, kind: ExprKind::Parameter(0) }],
                body: ExprId(0),
            };
            let diagnostics = verify(Program { functions: vec![function] }, &sources)
                .expect_err("reserved profile types must fail");
            assert!(codes(&diagnostics).contains(&"ZRYNA-I1006"));
        }
        let bool_literal = Function {
            name: "boolLiteral".to_owned(),
            parameters: Vec::new(),
            return_type: Type::Bool,
            expressions: vec![Expr { ty: Type::Bool, span, kind: ExprKind::BoolLiteral(true) }],
            body: ExprId(0),
        };
        let diagnostics = verify(Program { functions: vec![bool_literal] }, &sources)
            .expect_err("bool literal must remain outside I32V1");
        assert!(codes(&diagnostics).contains(&"ZRYNA-I1006"));

        let mistyped_bool_literal = Function {
            name: "mistypedBoolLiteral".to_owned(),
            parameters: Vec::new(),
            return_type: Type::I32,
            expressions: vec![Expr { ty: Type::I32, span, kind: ExprKind::BoolLiteral(false) }],
            body: ExprId(0),
        };
        assert!(
            codes(
                &verify(Program { functions: vec![mistyped_bool_literal] }, &sources)
                    .expect_err("mistyped bool literal must fail")
            )
            .contains(&"ZRYNA-I1005")
        );
        let function = Function {
            name: "badParameter".to_owned(),
            parameters: vec![Type::I32],
            return_type: Type::I32,
            expressions: vec![Expr { ty: Type::I32, span, kind: ExprKind::Parameter(1) }],
            body: ExprId(0),
        };
        assert!(
            codes(
                &verify(Program { functions: vec![function] }, &sources)
                    .expect_err("missing parameter must fail")
            )
            .contains(&"ZRYNA-I1003")
        );
    }

    #[test]
    fn rejects_invalid_body_result_literal_and_addition_types() {
        let sources = sources();
        let span = span(&sources);

        let mut missing_body = literal_function("missingBody", span);
        missing_body.body = ExprId(u32::MAX);
        assert!(
            codes(
                &verify(Program { functions: vec![missing_body] }, &sources)
                    .expect_err("missing body must fail")
            )
            .contains(&"ZRYNA-I1001")
        );

        let mut wrong_result = literal_function("wrongResult", span);
        wrong_result.return_type = Type::Bool;
        let result_diagnostics = verify(Program { functions: vec![wrong_result] }, &sources)
            .expect_err("mismatched result type must fail");
        assert!(codes(&result_diagnostics).contains(&"ZRYNA-I1002"));
        assert!(codes(&result_diagnostics).contains(&"ZRYNA-I1006"));

        let mistyped_literal = Function {
            name: "mistypedLiteral".to_owned(),
            parameters: Vec::new(),
            return_type: Type::Bool,
            expressions: vec![Expr { ty: Type::Bool, span, kind: ExprKind::I32Literal(1) }],
            body: ExprId(0),
        };
        let literal_diagnostics = verify(Program { functions: vec![mistyped_literal] }, &sources)
            .expect_err("mistyped literal must fail");
        assert!(codes(&literal_diagnostics).contains(&"ZRYNA-I1005"));
        assert!(codes(&literal_diagnostics).contains(&"ZRYNA-I1006"));

        let mistyped_addition = Function {
            name: "mistypedAddition".to_owned(),
            parameters: Vec::new(),
            return_type: Type::I32,
            expressions: vec![
                Expr { ty: Type::Bool, span, kind: ExprKind::I32Literal(1) },
                Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(2) },
                Expr {
                    ty: Type::I32,
                    span,
                    kind: ExprKind::I32Add { lhs: ExprId(0), rhs: ExprId(1) },
                },
            ],
            body: ExprId(2),
        };
        assert!(
            codes(
                &verify(Program { functions: vec![mistyped_addition] }, &sources)
                    .expect_err("mistyped addition must fail")
            )
            .contains(&"ZRYNA-I1004")
        );
    }

    #[test]
    fn resource_preflight_rejects_first_extra_before_proportional_work() {
        let sources = sources();
        let span = span(&sources);
        let mut too_many_parameters = literal_function("parameters", span);
        too_many_parameters.parameters = vec![Type::I32; MAX_IR_PARAMETERS_PER_FUNCTION + 1];
        assert_eq!(
            codes(
                &verify(Program { functions: vec![too_many_parameters] }, &sources)
                    .expect_err("parameter limit +1 must fail")
            ),
            vec!["ZRYNA-I1201"]
        );

        let expression = Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(1) };
        let mut too_many_expressions = literal_function("expressions", span);
        too_many_expressions.expressions = vec![expression; MAX_IR_EXPRESSIONS_PER_FUNCTION + 1];
        assert_eq!(
            codes(
                &verify(Program { functions: vec![too_many_expressions] }, &sources)
                    .expect_err("expression limit +1 must fail")
            ),
            vec!["ZRYNA-I1201"]
        );

        let function = literal_function("safeFunction", span);
        let program = Program { functions: vec![function; MAX_IR_FUNCTIONS + 1] };
        assert_eq!(
            codes(&verify(program, &sources).expect_err("function limit +1 must fail")),
            vec!["ZRYNA-I1201"]
        );
    }

    #[test]
    fn aggregate_resource_limits_accept_exact_and_reject_first_extra() {
        let sources = sources();
        let span = span(&sources);

        let mut parameter_function = literal_function("parameters", span);
        parameter_function.parameters = vec![Type::I32; MAX_IR_PARAMETERS_PER_FUNCTION];
        let exact_parameters = Program {
            functions: vec![
                parameter_function.clone();
                MAX_IR_PARAMETERS_PER_PROGRAM / MAX_IR_PARAMETERS_PER_FUNCTION
            ],
        };
        let mut errors = VerificationErrors::default();
        verify_resource_limits(&exact_parameters, &mut errors);
        assert!(errors.is_empty());
        let mut extra_parameters = exact_parameters;
        extra_parameters.functions.push(parameter_function);
        let mut errors = VerificationErrors::default();
        verify_resource_limits(&extra_parameters, &mut errors);
        assert_eq!(codes(&errors.finish()), vec!["ZRYNA-I1201"]);

        let expression = Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(1) };
        let mut expression_function = literal_function("expressions", span);
        expression_function.expressions = vec![expression; MAX_IR_EXPRESSIONS_PER_FUNCTION];
        let exact_expressions = Program {
            functions: vec![
                expression_function.clone();
                MAX_IR_EXPRESSIONS_PER_PROGRAM / MAX_IR_EXPRESSIONS_PER_FUNCTION
            ],
        };
        let mut errors = VerificationErrors::default();
        verify_resource_limits(&exact_expressions, &mut errors);
        assert!(errors.is_empty());
        let mut extra_expressions = exact_expressions;
        extra_expressions.functions.push(expression_function);
        let mut errors = VerificationErrors::default();
        verify_resource_limits(&extra_expressions, &mut errors);
        assert_eq!(codes(&errors.finish()), vec!["ZRYNA-I1201"]);
    }

    #[test]
    fn diagnostic_budget_is_bounded_and_terminal() {
        let sources = sources();
        let span = span(&sources);
        let expressions = (0..300)
            .map(|_| Expr { ty: Type::I32, span, kind: ExprKind::Parameter(u32::MAX) })
            .collect::<Vec<_>>();
        let function = Function {
            name: "manyErrors".to_owned(),
            parameters: Vec::new(),
            return_type: Type::I32,
            body: ExprId(299),
            expressions,
        };
        let diagnostics = verify(Program { functions: vec![function] }, &sources)
            .expect_err("diagnostic limit fixture must fail");
        assert_eq!(diagnostics.len(), MAX_IR_DIAGNOSTICS);
        assert_eq!(diagnostics.last().map(Diagnostic::code), Some("ZRYNA-I1202"));
    }
}
