//! Permanent boundary for Zryna-owned semantic analysis and IR lowering.

#![forbid(unsafe_code)]

pub mod command_h1_v1;
pub mod control_flow_v1;
pub mod data_ownership_v1;
pub mod definition_queries;

use std::collections::BTreeMap;
mod diagnostic_order;
use diagnostic_order::compare_diagnostics;

use zryna_abi::{AbiViolationKind, raw as raw_abi, verify_v1};
use zryna_diagnostics::{Diagnostic, Severity};
use zryna_ir::{Expr, ExprId, ExprKind, Function, Program, Type};
use zryna_source::{SourceMap, Span};
use zryna_syntax::v2::{
    ExpressionKind, ExpressionSyntax, FunctionBodySyntax, FunctionSyntax, ParameterSyntax,
    ProjectSyntaxSnapshot, StatementKind, TypeSyntax, TypeSyntaxKind,
};

/// Maximum retained semantic diagnostics, including the terminal budget diagnostic.
pub const MAX_SEMANTIC_DIAGNOSTICS: usize = 256;

const _: () = {
    assert!(zryna_syntax::v2::MAX_FUNCTIONS_PER_PROJECT <= zryna_ir::MAX_IR_FUNCTIONS);
    assert!(
        zryna_syntax::v2::MAX_PARAMETERS_PER_FUNCTION <= zryna_ir::MAX_IR_PARAMETERS_PER_FUNCTION
    );
    assert!(
        zryna_syntax::v2::MAX_PARAMETERS_PER_PROJECT <= zryna_ir::MAX_IR_PARAMETERS_PER_PROGRAM
    );
    assert!(
        zryna_syntax::v2::MAX_EXPRESSIONS_PER_FUNCTION <= zryna_ir::MAX_IR_EXPRESSIONS_PER_FUNCTION
    );
    assert!(
        zryna_syntax::v2::MAX_EXPRESSIONS_PER_PROJECT <= zryna_ir::MAX_IR_EXPRESSIONS_PER_PROGRAM
    );
    assert!(zryna_syntax::v2::MAX_EXPRESSION_DEPTH <= zryna_ir::MAX_IR_EXPRESSION_DEPTH);
};

/// Inputs that a future semantic implementation must consume without frontend authority.
///
/// Raw protocol-v2 claims cannot enter this boundary:
///
/// ```compile_fail
/// fn bypass<'a>(
///     raw: &'a zryna_syntax::v2::RawProjectSyntaxSnapshot,
///     sources: &'a zryna_source::SourceMap,
/// ) -> Option<zryna_semantics::SemanticInput<'a>> {
///     zryna_semantics::SemanticInput::try_new(raw, sources)
/// }
/// ```
///
/// The wrapper cannot be forged without [`SemanticInput::try_new`]:
///
/// ```compile_fail
/// fn forge<'a>(
///     syntax: &'a zryna_syntax::v2::ProjectSyntaxSnapshot,
///     sources: &'a zryna_source::SourceMap,
/// ) -> zryna_semantics::SemanticInput<'a> {
///     zryna_semantics::SemanticInput { syntax, sources }
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct SemanticInput<'a> {
    syntax: &'a ProjectSyntaxSnapshot,
    sources: &'a SourceMap,
}

impl<'a> SemanticInput<'a> {
    /// Binds verified syntax to the exact authoritative source map used to construct it.
    ///
    /// Returns `None` when the snapshot was verified by a different source-map instance or
    /// contains a provider error that must stop compilation before semantic analysis.
    #[must_use]
    pub fn try_new(syntax: &'a ProjectSyntaxSnapshot, sources: &'a SourceMap) -> Option<Self> {
        (syntax.is_bound_to(sources)
            && syntax
                .diagnostics()
                .iter()
                .all(|diagnostic| diagnostic.severity() != Severity::Error))
        .then_some(Self { syntax, sources })
    }

    /// Returns the verified provider-neutral syntax project.
    #[must_use]
    pub const fn syntax(self) -> &'a ProjectSyntaxSnapshot {
        self.syntax
    }

    /// Returns the authoritative source map for semantic diagnostics.
    #[must_use]
    pub const fn sources(self) -> &'a SourceMap {
        self.sources
    }
}

/// Output contract for strict semantic analysis and lowering.
pub type SemanticResult = Result<Program, Vec<Diagnostic>>;

/// Resolves names, checks the strict source subset, and lowers it to unverified Universal IR.
///
/// This phase owns source-language meaning. It accepts only verified provider-neutral syntax and
/// never consumes provider syntax kinds, node identities, symbols, or inferred types. The returned
/// [`Program`] remains untrusted until `zryna_ir::verify` accepts it.
///
/// # Errors
///
/// Returns deterministic, bounded, source-located diagnostics for every semantic rejection.
pub fn lower(input: SemanticInput<'_>) -> SemanticResult {
    let mut errors = SemanticErrors::default();
    match input.syntax().files() {
        [] => errors.push(Diagnostic::error(
            "ZRYNA-M1013",
            None,
            "the first strict source subset requires one source entrypoint",
            "compile exactly one source file until module resolution is enabled",
        )),
        [file] if file.functions().is_empty() => errors.push(Diagnostic::error(
            "ZRYNA-M1014",
            Some(file.path().as_str().to_owned()),
            "the source entrypoint contains no exported function",
            "declare at least one explicitly typed exported function",
        )),
        [_] => {}
        _ => errors.push(Diagnostic::error(
            "ZRYNA-M1013",
            None,
            "multiple source files require module semantics that are not enabled",
            "compile exactly one source entrypoint in the first strict source subset",
        )),
    }
    verify_export_names(input.syntax(), &mut errors);

    let mut functions = Vec::new();
    for file in input.syntax().files() {
        for function in file.functions() {
            if let Some(function) = lower_function(function, &mut errors) {
                functions.push(function);
            }
        }
    }

    if errors.is_empty() { Ok(Program { functions }) } else { Err(errors.finish()) }
}

fn verify_export_names(syntax: &ProjectSyntaxSnapshot, errors: &mut SemanticErrors) {
    let functions =
        syntax.files().iter().flat_map(zryna_syntax::v2::SourceUnit::functions).collect::<Vec<_>>();
    let exports = functions
        .iter()
        .map(|function| {
            raw_abi::Export::new(
                function.name().text().to_owned(),
                raw_abi::Signature::new(Vec::new(), raw_abi::Type::I32),
            )
        })
        .collect();
    let Err(violations) = verify_v1(raw_abi::Module::new(exports)) else {
        return;
    };
    for violation in violations {
        let function = violation.export_index().and_then(|index| functions.get(index)).copied();
        match violation.kind() {
            AbiViolationKind::InvalidLogicalName => {
                if let Some(function) = function {
                    errors.push(Diagnostic::error_at(
                        "ZRYNA-M1011",
                        function.name().span(),
                        format!(
                            "export '{}' is not a valid scalar ABI logical name",
                            function.name().text()
                        ),
                        "use 1 to 128 ASCII bytes matching [A-Za-z_][A-Za-z0-9_]* and avoid reserved bindings",
                    ));
                }
            }
            AbiViolationKind::DuplicateLogicalName { first_index } => {
                if let Some(function) = function {
                    errors.push(Diagnostic::error_at(
                        "ZRYNA-M1001",
                        function.name().span(),
                        format!(
                            "export '{}' duplicates export #{first_index}",
                            function.name().text()
                        ),
                        "give every exported function one exact unique source name",
                    ));
                }
            }
            AbiViolationKind::PortableNameCollision { first_index } => {
                if let Some(function) = function {
                    errors.push(Diagnostic::error_at(
                        "ZRYNA-M1012",
                        function.name().span(),
                        format!(
                            "export '{}' collides with export #{first_index} under the portable target identity",
                            function.name().text()
                        ),
                        "choose export names that remain unique when ASCII case is ignored",
                    ));
                }
            }
            AbiViolationKind::TooManyExports
            | AbiViolationKind::TooManyParameters
            | AbiViolationKind::TooManyParametersInModule => errors.push(Diagnostic::error(
                "ZRYNA-M1201",
                None,
                "semantic export declarations exceed the scalar ABI resource limits",
                "reduce the source before semantic analysis",
            )),
            AbiViolationKind::ViolationBudgetExceeded => errors.push(Diagnostic::error(
                "ZRYNA-M1201",
                None,
                "semantic export-name diagnostics exceeded their deterministic limit",
                "fix earlier export declarations before compiling again",
            )),
            AbiViolationKind::UnsupportedScalarType => {}
        }
    }
}

type ParameterBindings = BTreeMap<String, (u32, Option<Type>)>;
type LoweredExpressions = (Vec<Option<Type>>, Vec<Option<Expr>>);

fn lower_function(syntax: &FunctionSyntax, errors: &mut SemanticErrors) -> Option<Function> {
    let (parameters, bindings) = lower_parameters(syntax.parameters(), errors);
    let return_type = lower_type(syntax.result_type(), "result", errors);
    let body_id = select_body_id(syntax.body(), errors);
    let (expression_types, expressions) = lower_expressions(syntax.body(), &bindings, errors);
    check_return_type(syntax.body(), return_type, body_id, &expression_types, errors);

    Some(Function {
        name: syntax.name().text().to_owned(),
        parameters: parameters.into_iter().collect::<Option<Vec<_>>>()?,
        return_type: return_type?,
        expressions: expressions.into_iter().collect::<Option<Vec<_>>>()?,
        body: body_id?,
    })
}

fn lower_parameters(
    parameters: &[ParameterSyntax],
    errors: &mut SemanticErrors,
) -> (Vec<Option<Type>>, ParameterBindings) {
    let mut types = Vec::with_capacity(parameters.len());
    let mut bindings = ParameterBindings::new();
    for (index, parameter) in parameters.iter().enumerate() {
        let ty = lower_type(parameter.type_syntax(), "parameter", errors);
        types.push(ty);
        let Some(index) = u32::try_from(index).ok() else {
            errors.push(internal_limit_error(parameter.span()));
            continue;
        };
        if bindings.insert(parameter.name().text().to_owned(), (index, ty)).is_some() {
            errors.push(Diagnostic::error_at(
                "ZRYNA-M1002",
                parameter.name().span(),
                format!("parameter '{}' is declared more than once", parameter.name().text()),
                "give every parameter one exact unique name within its function",
            ));
        }
    }
    (types, bindings)
}

fn select_body_id(body: &FunctionBodySyntax, errors: &mut SemanticErrors) -> Option<ExprId> {
    let statements = body.statements();
    if let [statement] = statements {
        let StatementKind::Return { value, .. } = statement.kind();
        Some(ExprId(value.index()))
    } else if statements.is_empty() {
        errors.push(Diagnostic::error_at(
            "ZRYNA-M1009",
            body.span(),
            "function body has no value return",
            "write exactly one return statement in the first strict source subset",
        ));
        None
    } else {
        errors.push(Diagnostic::error_at(
            "ZRYNA-M1009",
            statements[1].span(),
            "function body has more than one statement",
            "write exactly one return statement until control flow is enabled",
        ));
        None
    }
}

fn lower_expressions(
    body: &FunctionBodySyntax,
    bindings: &ParameterBindings,
    errors: &mut SemanticErrors,
) -> LoweredExpressions {
    let mut types = Vec::with_capacity(body.expressions().len());
    let mut expressions = Vec::with_capacity(body.expressions().len());
    for expression in body.expressions() {
        let lowered = lower_expression(expression, &types, bindings, errors);
        types.push(lowered.as_ref().map(|(ty, _)| *ty));
        expressions.push(lowered.map(|(ty, kind)| Expr { ty, span: expression.span(), kind }));
    }
    (types, expressions)
}

fn lower_expression(
    expression: &ExpressionSyntax,
    prior_types: &[Option<Type>],
    bindings: &ParameterBindings,
    errors: &mut SemanticErrors,
) -> Option<(Type, ExprKind)> {
    match expression.kind() {
        ExpressionKind::Reference { name } => match bindings.get(name.text()).copied() {
            Some((index, Some(ty))) => Some((ty, ExprKind::Parameter(index))),
            Some((_, None)) => None,
            None => {
                errors.push(Diagnostic::error_at(
                    "ZRYNA-M1006",
                    name.span(),
                    format!("name '{}' is not declared in this function", name.text()),
                    "reference one of the function's explicitly typed parameters",
                ));
                None
            }
        },
        ExpressionKind::BoolLiteral { value } => Some((Type::Bool, ExprKind::BoolLiteral(*value))),
        ExpressionKind::I32Literal { spelling } => lower_i32(expression, spelling, errors),
        ExpressionKind::Addition { operator_span, lhs, rhs } => {
            let lhs_type = expression_type(prior_types, lhs.index());
            let rhs_type = expression_type(prior_types, rhs.index());
            if lhs_type == Some(Type::I32) && rhs_type == Some(Type::I32) {
                Some((
                    Type::I32,
                    ExprKind::I32Add { lhs: ExprId(lhs.index()), rhs: ExprId(rhs.index()) },
                ))
            } else {
                errors.push(Diagnostic::error_at(
                    "ZRYNA-M1008",
                    *operator_span,
                    "addition requires two i32 operands",
                    "use '+' only with values whose exact Zryna type is i32",
                ));
                None
            }
        }
    }
}

fn lower_i32(
    expression: &ExpressionSyntax,
    spelling: &str,
    errors: &mut SemanticErrors,
) -> Option<(Type, ExprKind)> {
    if let Ok(value) = spelling.parse::<i32>() {
        Some((Type::I32, ExprKind::I32Literal(value)))
    } else {
        errors.push(Diagnostic::error_at(
            "ZRYNA-M1007",
            expression.span(),
            format!("integer literal '{spelling}' is outside the i32 range"),
            "use a decimal integer from -2147483648 through 2147483647",
        ));
        None
    }
}

fn check_return_type(
    body: &FunctionBodySyntax,
    expected: Option<Type>,
    body_id: Option<ExprId>,
    expression_types: &[Option<Type>],
    errors: &mut SemanticErrors,
) {
    let Some((body_id, actual)) = body_id.and_then(|body_id| {
        expression_type(expression_types, body_id.0).map(|actual| (body_id, actual))
    }) else {
        return;
    };
    if Some(actual) == expected {
        return;
    }
    let span = usize::try_from(body_id.0)
        .ok()
        .and_then(|index| body.expressions().get(index))
        .map_or(body.span(), ExpressionSyntax::span);
    errors.push(Diagnostic::error_at(
        "ZRYNA-M1010",
        span,
        "returned expression does not match the declared result type",
        "return a value with exactly the function's declared result type",
    ));
}

fn lower_type(
    syntax: &TypeSyntax,
    position: &'static str,
    errors: &mut SemanticErrors,
) -> Option<Type> {
    match syntax.kind() {
        TypeSyntaxKind::Missing => {
            errors.push(Diagnostic::error_at(
                "ZRYNA-M1003",
                syntax.span(),
                format!("{position} type annotation is required"),
                "write an explicit i32 or bool annotation; implicit any is unavailable",
            ));
            None
        }
        TypeSyntaxKind::Named { name } if name == "any" => {
            errors.push(Diagnostic::error_at(
                "ZRYNA-M1004",
                syntax.span(),
                format!("{position} type 'any' is not part of Zryna"),
                "replace any with an exact supported Zryna type",
            ));
            None
        }
        TypeSyntaxKind::Named { name } if name == "i32" => Some(Type::I32),
        TypeSyntaxKind::Named { name } if name == "bool" => Some(Type::Bool),
        TypeSyntaxKind::Named { name } => {
            errors.push(Diagnostic::error_at(
                "ZRYNA-M1005",
                syntax.span(),
                format!("{position} type '{name}' is not supported in this source subset"),
                "use only i32 or bool in the first strict source subset",
            ));
            None
        }
    }
}

fn expression_type(types: &[Option<Type>], index: u32) -> Option<Type> {
    usize::try_from(index).ok().and_then(|index| types.get(index)).copied().flatten()
}

fn internal_limit_error(span: Span) -> Diagnostic {
    Diagnostic::error_at(
        "ZRYNA-M1201",
        span,
        "semantic input exceeded an internal identifier limit",
        "reduce the source before semantic analysis",
    )
}

#[derive(Default)]
struct SemanticErrors {
    diagnostics: Vec<Diagnostic>,
    exhausted: bool,
}

impl SemanticErrors {
    fn push(&mut self, diagnostic: Diagnostic) {
        if self.exhausted {
            return;
        }
        if self.diagnostics.len() < MAX_SEMANTIC_DIAGNOSTICS.saturating_sub(1) {
            self.diagnostics.push(diagnostic);
            return;
        }
        self.diagnostics.push(Diagnostic::error(
            "ZRYNA-M1201",
            None,
            format!("semantic analysis reached its diagnostic limit of {MAX_SEMANTIC_DIAGNOSTICS}"),
            "fix the retained diagnostics before compiling again",
        ));
        self.exhausted = true;
    }

    fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    fn finish(mut self) -> Vec<Diagnostic> {
        self.diagnostics.sort_by(compare_diagnostics);
        self.diagnostics
    }
}

#[cfg(test)]
mod tests;
