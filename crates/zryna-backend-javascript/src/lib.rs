//! Direct JavaScript emission from verified Zryna IR.
#![forbid(unsafe_code)]
use std::fmt::Write;

use zryna_diagnostics::Diagnostic;
use zryna_ir::control_flow_v1::{
    FunctionIdentity, ValueIdentity, VerifiedFunction as VerifiedControlFlowFunction,
    VerifiedInstructionKind, VerifiedProgram as VerifiedControlFlowProgram, VerifiedTerminatorKind,
};
use zryna_ir::{ExprKind, Type, VerifiedFunction, VerifiedProgram};
mod data_ownership_v1;
pub use data_ownership_v1::emit_data_ownership;

mod prelude;
use prelude::{JAVASCRIPT_PRELUDE, MAX_CONTROL_FLOW_JAVASCRIPT_BYTES};
pub mod generic_copy_v1;

/// JavaScript artifacts produced by one compilation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JavaScriptArtifact {
    /// ECMAScript module source.
    pub source: String,
}

/// Emits modern ECMAScript from verified Zryna IR.
///
/// Raw Universal IR is not accepted by this boundary:
///
/// ```compile_fail
/// let raw = zryna_ir::Program::default();
/// let _ = zryna_backend_javascript::emit(&raw);
/// ```
///
/// The separate raw `ControlFlowV1` profile cannot satisfy this M1 backend boundary either:
///
/// ```compile_fail
/// let raw = zryna_ir::control_flow_v1::raw::Program {
///     entry_module: zryna_ir::control_flow_v1::raw::ModuleId(0),
///     modules: Vec::new(),
/// };
/// let _ = zryna_backend_javascript::emit(&raw);
/// ```
///
/// # Errors
///
/// Returns a compiler diagnostic when a verified expression cannot be emitted.
pub fn emit(program: &VerifiedProgram) -> Result<JavaScriptArtifact, Diagnostic> {
    let mut output = String::new();
    if program.functions().len() == 0 {
        output.push_str("export {};\n");
        return Ok(JavaScriptArtifact { source: output });
    }

    output.push_str(JAVASCRIPT_PRELUDE);
    for (index, function) in program.functions().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        emit_function(function, &mut output)?;
    }
    Ok(JavaScriptArtifact { source: output })
}

/// Emits deterministic ECMAScript from a verified `ControlFlowV1` program.
///
/// This is an internal M2 backend boundary. It does not activate the M2 profile in the public
/// driver or CLI. Raw control-flow IR cannot be passed to this function:
///
/// ```compile_fail
/// let raw = zryna_ir::control_flow_v1::raw::Program {
///     entry_module: zryna_ir::control_flow_v1::raw::ModuleId(0),
///     modules: Vec::new(),
/// };
/// let _ = zryna_backend_javascript::emit_control_flow(&raw);
/// ```
///
/// # Errors
///
/// Returns a compiler diagnostic if deterministic source formatting fails.
pub fn emit_control_flow(
    program: &VerifiedControlFlowProgram,
) -> Result<JavaScriptArtifact, Diagnostic> {
    emit_control_flow_with_budget(program, MAX_CONTROL_FLOW_JAVASCRIPT_BYTES)
}

fn emit_control_flow_with_budget(
    program: &VerifiedControlFlowProgram,
    byte_budget: usize,
) -> Result<JavaScriptArtifact, Diagnostic> {
    let mut counter = ByteCounter::default();
    render_control_flow(program, &mut counter)?;
    if counter.bytes > byte_budget {
        return Err(emission_budget_error(byte_budget));
    }
    let mut output = String::new();
    output.try_reserve(counter.bytes).map_err(|_| {
        Diagnostic::error(
            "ZRYNA-J2003",
            None,
            "could not reserve the bounded deterministic JavaScript artifact",
            "reduce the verified ControlFlowV1 program below the JavaScript artifact budget",
        )
    })?;
    render_control_flow(program, &mut output)?;
    debug_assert_eq!(output.len(), counter.bytes);
    Ok(JavaScriptArtifact { source: output })
}

#[derive(Default)]
struct ByteCounter {
    bytes: usize,
}

impl Write for ByteCounter {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.bytes = self.bytes.checked_add(value.len()).ok_or(std::fmt::Error)?;
        Ok(())
    }
}

fn render_control_flow(
    program: &VerifiedControlFlowProgram,
    output: &mut impl Write,
) -> Result<(), Diagnostic> {
    let function_count = program.modules().map(|module| module.functions().len()).sum::<usize>();
    if function_count == 0 {
        output.write_str("export {};\n").map_err(control_flow_global_formatting_error)?;
        return Ok(());
    }

    output.write_str(JAVASCRIPT_PRELUDE).map_err(control_flow_global_formatting_error)?;
    let mut first = true;
    for module in program.modules() {
        for function in module.functions() {
            if !first {
                output.write_char('\n').map_err(control_flow_global_formatting_error)?;
            }
            first = false;
            emit_control_flow_function(function, output)?;
        }
    }

    let mut export_index = 0_usize;
    for module in program.modules() {
        for function in module.functions() {
            let Some(public_export) = function.public_export() else {
                continue;
            };
            output
                .write_char('\n')
                .map_err(|error| control_flow_formatting_error(function.id(), error))?;
            emit_public_wrapper(function, export_index, output)?;
            writeln!(
                output,
                "export {{ $zryna$e{export_index} as {} }};",
                public_export.javascript_name().as_str()
            )
            .map_err(|error| control_flow_formatting_error(function.id(), error))?;
            export_index += 1;
        }
    }
    if export_index == 0 {
        output.write_str("\nexport {};\n").map_err(control_flow_global_formatting_error)?;
    }
    Ok(())
}

fn emission_budget_error(byte_budget: usize) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-J2003",
        None,
        format!("deterministic JavaScript artifact exceeds its {byte_budget} byte emission budget"),
        "reduce the verified ControlFlowV1 program below the JavaScript artifact budget",
    )
}

fn emit_control_flow_function(
    function: VerifiedControlFlowFunction<'_>,
    output: &mut impl Write,
) -> Result<(), Diagnostic> {
    let id = function.id();
    write!(output, "function {}(", private_function_name(id))
        .map_err(|error| control_flow_formatting_error(id, error))?;
    let parameters = function.parameters().collect::<Vec<_>>();
    for index in 0..parameters.len() {
        if index > 0 {
            output.write_str(", ").map_err(|error| control_flow_formatting_error(id, error))?;
        }
        write!(output, "p{index}").map_err(|error| control_flow_formatting_error(id, error))?;
    }
    output.write_str(") {\n").map_err(|error| control_flow_formatting_error(id, error))?;

    let blocks = function.blocks().collect::<Vec<_>>();
    let block_parameters = blocks
        .iter()
        .map(|block| block.parameters().map(|(value, _, _)| value).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let value_count = parameters.len()
        + blocks
            .iter()
            .map(|block| block.parameters().len() + block.instructions().len())
            .sum::<usize>();
    for value in 0..value_count {
        writeln!(output, "  let v{value};")
            .map_err(|error| control_flow_formatting_error(id, error))?;
    }
    let edge_temporary_count = block_parameters.iter().map(Vec::len).max().unwrap_or(0);
    for temporary in 0..edge_temporary_count {
        writeln!(output, "  let $zryna$a{temporary};")
            .map_err(|error| control_flow_formatting_error(id, error))?;
    }
    for (index, (value, _, _)) in parameters.iter().enumerate() {
        writeln!(output, "  v{} = p{index};", value.index())
            .map_err(|error| control_flow_formatting_error(id, error))?;
    }
    output
        .write_str("  let $zryna$block = 0;\n  while (true) {\n    switch ($zryna$block) {\n")
        .map_err(|error| control_flow_formatting_error(id, error))?;

    for block in &blocks {
        writeln!(output, "      case {}:", block.id().index())
            .map_err(|error| control_flow_formatting_error(id, error))?;
        for instruction in block.instructions() {
            write!(output, "        v{} = ", instruction.result().index())
                .map_err(|error| control_flow_formatting_error(id, error))?;
            emit_control_flow_instruction(instruction.kind(), output)
                .map_err(|error| control_flow_formatting_error(id, error))?;
            output.write_str(";\n").map_err(|error| control_flow_formatting_error(id, error))?;
        }
        emit_terminator(block.terminator().kind(), &block_parameters, output, id, "        ")?;
    }
    output
        .write_str("      default:\n        throw new Error(\"ZRYNA-J2001: invalid verified control-flow state\");\n    }\n  }\n}\n")
        .map_err(|error| control_flow_formatting_error(id, error))?;
    Ok(())
}

fn emit_control_flow_instruction(
    kind: VerifiedInstructionKind<'_>,
    output: &mut impl Write,
) -> std::fmt::Result {
    match kind {
        VerifiedInstructionKind::BoolLiteral(value) => {
            output.write_str(if value { "true" } else { "false" })?;
        }
        VerifiedInstructionKind::I32Literal(value) => write!(output, "{value}")?,
        VerifiedInstructionKind::I32Add(lhs, rhs) => {
            write!(output, "(v{} + v{}) | 0", lhs.index(), rhs.index())?;
        }
        VerifiedInstructionKind::I32Sub(lhs, rhs) => {
            write!(output, "(v{} - v{}) | 0", lhs.index(), rhs.index())?;
        }
        VerifiedInstructionKind::I32Mul(lhs, rhs) => {
            write!(output, "$zryna$imul(v{}, v{})", lhs.index(), rhs.index())?;
        }
        VerifiedInstructionKind::I32Neg(operand) => {
            write!(output, "(0 - v{}) | 0", operand.index())?;
        }
        VerifiedInstructionKind::Eq(lhs, rhs) => {
            write!(output, "v{} === v{}", lhs.index(), rhs.index())?;
        }
        VerifiedInstructionKind::Ne(lhs, rhs) => {
            write!(output, "v{} !== v{}", lhs.index(), rhs.index())?;
        }
        VerifiedInstructionKind::I32LtS(lhs, rhs) => {
            write!(output, "v{} < v{}", lhs.index(), rhs.index())?;
        }
        VerifiedInstructionKind::I32LeS(lhs, rhs) => {
            write!(output, "v{} <= v{}", lhs.index(), rhs.index())?;
        }
        VerifiedInstructionKind::I32GtS(lhs, rhs) => {
            write!(output, "v{} > v{}", lhs.index(), rhs.index())?;
        }
        VerifiedInstructionKind::I32GeS(lhs, rhs) => {
            write!(output, "v{} >= v{}", lhs.index(), rhs.index())?;
        }
        VerifiedInstructionKind::DirectCall { callee, arguments } => {
            write!(output, "{}(", private_function_name(callee))?;
            for (index, argument) in arguments.iter().enumerate() {
                if index > 0 {
                    output.write_str(", ")?;
                }
                write!(output, "v{}", argument.index())?;
            }
            output.write_char(')')?;
        }
    }
    Ok(())
}

fn emit_terminator(
    kind: VerifiedTerminatorKind<'_>,
    block_parameters: &[Vec<ValueIdentity>],
    output: &mut impl Write,
    function: FunctionIdentity,
    indent: &str,
) -> Result<(), Diagnostic> {
    match kind {
        VerifiedTerminatorKind::Return(value) => {
            writeln!(output, "{indent}return v{};", value.index())
                .map_err(|error| control_flow_formatting_error(function, error))?;
        }
        VerifiedTerminatorKind::Jump { target, arguments } => {
            emit_edge(
                target.index(),
                arguments.iter(),
                block_parameters,
                output,
                function,
                indent,
            )?;
        }
        VerifiedTerminatorKind::Branch {
            condition,
            true_target,
            true_arguments,
            false_target,
            false_arguments,
        } => {
            writeln!(output, "{indent}if (v{} === true) {{", condition.index())
                .map_err(|error| control_flow_formatting_error(function, error))?;
            emit_edge(
                true_target.index(),
                true_arguments.iter(),
                block_parameters,
                output,
                function,
                &format!("{indent}  "),
            )?;
            writeln!(output, "{indent}}} else {{")
                .map_err(|error| control_flow_formatting_error(function, error))?;
            emit_edge(
                false_target.index(),
                false_arguments.iter(),
                block_parameters,
                output,
                function,
                &format!("{indent}  "),
            )?;
            writeln!(output, "{indent}}}")
                .map_err(|error| control_flow_formatting_error(function, error))?;
        }
    }
    Ok(())
}

fn emit_edge(
    target: u32,
    arguments: impl ExactSizeIterator<Item = ValueIdentity>,
    block_parameters: &[Vec<ValueIdentity>],
    output: &mut impl Write,
    function: FunctionIdentity,
    indent: &str,
) -> Result<(), Diagnostic> {
    for (index, argument) in arguments.enumerate() {
        writeln!(output, "{indent}$zryna$a{index} = v{};", argument.index())
            .map_err(|error| control_flow_formatting_error(function, error))?;
    }
    for (index, parameter) in block_parameters[target as usize].iter().enumerate() {
        writeln!(output, "{indent}v{} = $zryna$a{index};", parameter.index())
            .map_err(|error| control_flow_formatting_error(function, error))?;
    }
    writeln!(output, "{indent}$zryna$block = {target};\n{indent}continue;")
        .map_err(|error| control_flow_formatting_error(function, error))?;
    Ok(())
}

fn emit_public_wrapper(
    function: VerifiedControlFlowFunction<'_>,
    export_index: usize,
    output: &mut impl Write,
) -> Result<(), Diagnostic> {
    let id = function.id();
    let parameters = function.parameters().collect::<Vec<_>>();
    write!(output, "function $zryna$e{export_index}(")
        .map_err(|error| control_flow_formatting_error(id, error))?;
    for index in 0..parameters.len() {
        if index > 0 {
            output.write_str(", ").map_err(|error| control_flow_formatting_error(id, error))?;
        }
        write!(output, "p{index}").map_err(|error| control_flow_formatting_error(id, error))?;
    }
    output.write_str(") {\n").map_err(|error| control_flow_formatting_error(id, error))?;
    writeln!(output, "  $zryna$checkArity(arguments.length, {});", parameters.len())
        .map_err(|error| control_flow_formatting_error(id, error))?;
    for (index, (_, ty, _)) in parameters.iter().enumerate() {
        let validator = scalar_validator(*ty).ok_or_else(|| control_flow_profile_error(id))?;
        writeln!(output, "  p{index} = {validator}(p{index});")
            .map_err(|error| control_flow_formatting_error(id, error))?;
    }
    let result_validator =
        scalar_validator(function.result()).ok_or_else(|| control_flow_profile_error(id))?;
    write!(output, "  return {}({}(", result_validator, private_function_name(id))
        .map_err(|error| control_flow_formatting_error(id, error))?;
    for index in 0..parameters.len() {
        if index > 0 {
            output.write_str(", ").map_err(|error| control_flow_formatting_error(id, error))?;
        }
        write!(output, "p{index}").map_err(|error| control_flow_formatting_error(id, error))?;
    }
    output.write_str("));\n}\n").map_err(|error| control_flow_formatting_error(id, error))?;
    Ok(())
}

fn private_function_name(id: FunctionIdentity) -> String {
    format!("$zryna$m{}f{}", id.module().index(), id.declaration())
}

const fn scalar_validator(ty: Type) -> Option<&'static str> {
    match ty {
        Type::I32 => Some("$zryna$i32"),
        Type::Bool => Some("$zryna$bool"),
        Type::Unit => None,
    }
}

fn control_flow_profile_error(function: FunctionIdentity) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-J2001",
        None,
        format!(
            "verified function {}:{} contains a type outside the JavaScript proof profile",
            function.module().index(),
            function.declaration()
        ),
        "report this compiler invariant failure with the smallest reproducible Zryna source",
    )
}

fn control_flow_formatting_error(function: FunctionIdentity, error: std::fmt::Error) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-J2002",
        None,
        format!(
            "could not emit JavaScript for verified function {}:{}: {error}",
            function.module().index(),
            function.declaration()
        ),
        "report this compiler failure with the smallest reproducible Zryna source",
    )
}

fn control_flow_global_formatting_error(error: std::fmt::Error) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-J2002",
        None,
        format!("could not emit deterministic JavaScript: {error}"),
        "report this compiler failure with the smallest reproducible Zryna source",
    )
}

fn emit_function(function: VerifiedFunction<'_>, output: &mut String) -> Result<(), Diagnostic> {
    write!(output, "export function {}(", function.abi_export().javascript_name().as_str())
        .map_err(|error| formatting_error(function, error))?;
    for index in 0..function.parameters().len() {
        if index > 0 {
            output.push_str(", ");
        }
        write!(output, "p{index}").map_err(|error| formatting_error(function, error))?;
    }
    output.push_str(") {\n");
    writeln!(output, "  $zryna$checkArity(arguments.length, {});", function.parameters().len())
        .map_err(|error| formatting_error(function, error))?;
    for (index, parameter) in function.parameters().iter().enumerate() {
        if *parameter != Type::I32 {
            return Err(profile_invariant_error(function));
        }
        writeln!(output, "  p{index} = $zryna$i32(p{index});")
            .map_err(|error| formatting_error(function, error))?;
    }
    for (index, expression) in function.expressions().iter().enumerate() {
        write!(output, "  const v{index} = ").map_err(|error| formatting_error(function, error))?;
        match &expression.kind {
            ExprKind::Parameter(parameter) => {
                write!(output, "p{parameter}")
                    .map_err(|error| formatting_error(function, error))?;
            }
            ExprKind::BoolLiteral(_) => {
                return Err(profile_invariant_error(function));
            }
            ExprKind::I32Add { lhs, rhs } => {
                write!(output, "(v{} + v{}) | 0", lhs.0, rhs.0)
                    .map_err(|error| formatting_error(function, error))?;
            }
            ExprKind::I32Literal(value) => {
                write!(output, "{value}").map_err(|error| formatting_error(function, error))?;
            }
        }
        output.push_str(";\n");
    }
    if function.return_type() != Type::I32 {
        return Err(profile_invariant_error(function));
    }
    write!(output, "  return $zryna$i32(v{});\n}}\n", function.body().0)
        .map_err(|error| formatting_error(function, error))?;
    Ok(())
}

fn profile_invariant_error(function: VerifiedFunction<'_>) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-J1001",
        None,
        format!(
            "verified function '{}' contains an operation outside the JavaScript proof profile",
            function.abi_export().javascript_name().as_str()
        ),
        "report this compiler invariant failure with the smallest reproducible Zryna source",
    )
}

fn formatting_error(function: VerifiedFunction<'_>, error: std::fmt::Error) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-J1002",
        None,
        format!(
            "could not emit JavaScript for '{}': {error}",
            function.abi_export().javascript_name().as_str()
        ),
        "report this compiler failure with the smallest reproducible Zryna source",
    )
}

/// Returns the JavaScript spelling associated with an IR type.
///
/// This representation helper does not admit a type into the current universal profile.
#[must_use]
pub const fn javascript_type(ty: Type) -> &'static str {
    match ty {
        Type::Unit => "undefined",
        Type::Bool => "boolean",
        Type::I32 => "number",
    }
}

#[cfg(test)]
mod tests {
    use zryna_ir::control_flow_v1::{self, raw as control_flow_raw};
    use zryna_ir::{
        Expr, ExprId, ExprKind, Function, MAX_IR_EXPRESSION_DEPTH, Program, Type, verify,
    };
    use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap, Span};

    fn control_flow_value(id: u32, ty: Type, span: Span) -> control_flow_raw::ValueDefinition {
        control_flow_raw::ValueDefinition { id: control_flow_raw::ValueId(id), ty, span }
    }

    fn control_flow_instruction(
        id: u32,
        ty: Type,
        span: Span,
        kind: control_flow_raw::InstructionKind,
    ) -> control_flow_raw::Instruction {
        control_flow_raw::Instruction { result: control_flow_value(id, ty, span), kind }
    }

    fn control_flow_terminator(
        span: Span,
        kind: control_flow_raw::Terminator,
    ) -> Vec<control_flow_raw::SpannedTerminator> {
        vec![control_flow_raw::SpannedTerminator { span, kind }]
    }

    #[allow(clippy::too_many_lines)]
    fn control_flow_fixture() -> (control_flow_v1::VerifiedProgram, SourceMap) {
        use control_flow_raw::{
            Block, BlockId, Function, FunctionId, InstructionKind as I, Module, ModuleId,
            Terminator as T, ValueId,
        };

        let sources = SourceMap::build(vec![SourceFileInput {
            path: "src/main.zry".to_owned(),
            text: "x".to_owned(),
        }])
        .expect("control-flow fixture source map");
        let path = NormalizedSourcePath::new("src/main.zry").expect("fixture path");
        let file = sources.file_id(&path).expect("fixture file");
        let span = sources.span(file, 0, 1).expect("fixture span");
        let binary = Function {
            id: FunctionId { module: ModuleId(0), declaration: 0 },
            entry_export: None,
            span,
            parameters: vec![
                control_flow_value(0, Type::I32, span),
                control_flow_value(1, Type::I32, span),
            ],
            result: Type::I32,
            blocks: vec![Block {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![control_flow_instruction(
                    2,
                    Type::I32,
                    span,
                    I::I32Add { lhs: ValueId(0), rhs: ValueId(1) },
                )],
                terminators: control_flow_terminator(span, T::Return(ValueId(2))),
            }],
        };
        let operations = Function {
            id: FunctionId { module: ModuleId(0), declaration: 1 },
            entry_export: Some("Math".to_owned()),
            span,
            parameters: vec![
                control_flow_value(0, Type::Bool, span),
                control_flow_value(1, Type::I32, span),
                control_flow_value(2, Type::I32, span),
            ],
            result: Type::I32,
            blocks: vec![
                Block {
                    id: BlockId(0),
                    parameters: Vec::new(),
                    instructions: vec![
                        control_flow_instruction(3, Type::Bool, span, I::BoolLiteral(true)),
                        control_flow_instruction(4, Type::I32, span, I::I32Literal(i32::MIN)),
                        control_flow_instruction(
                            5,
                            Type::I32,
                            span,
                            I::I32Add { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            6,
                            Type::I32,
                            span,
                            I::I32Sub { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            7,
                            Type::I32,
                            span,
                            I::I32Mul { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            8,
                            Type::I32,
                            span,
                            I::I32Neg { operand: ValueId(4) },
                        ),
                        control_flow_instruction(
                            9,
                            Type::Bool,
                            span,
                            I::Eq { lhs: ValueId(0), rhs: ValueId(3) },
                        ),
                        control_flow_instruction(
                            10,
                            Type::Bool,
                            span,
                            I::Ne { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            11,
                            Type::Bool,
                            span,
                            I::I32LtS { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            12,
                            Type::Bool,
                            span,
                            I::I32LeS { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            13,
                            Type::Bool,
                            span,
                            I::I32GtS { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            14,
                            Type::Bool,
                            span,
                            I::I32GeS { lhs: ValueId(1), rhs: ValueId(2) },
                        ),
                        control_flow_instruction(
                            15,
                            Type::I32,
                            span,
                            I::DirectCall {
                                callee: FunctionId { module: ModuleId(0), declaration: 0 },
                                arguments: vec![ValueId(1), ValueId(2)],
                            },
                        ),
                    ],
                    terminators: control_flow_terminator(
                        span,
                        T::Branch {
                            condition: ValueId(0),
                            true_target: BlockId(1),
                            true_arguments: vec![ValueId(7)],
                            false_target: BlockId(1),
                            false_arguments: vec![ValueId(15)],
                        },
                    ),
                },
                Block {
                    id: BlockId(1),
                    parameters: vec![control_flow_value(16, Type::I32, span)],
                    instructions: Vec::new(),
                    terminators: control_flow_terminator(span, T::Return(ValueId(16))),
                },
            ],
        };
        let swap_loop = Function {
            id: FunctionId { module: ModuleId(0), declaration: 2 },
            entry_export: Some("Object".to_owned()),
            span,
            parameters: vec![
                control_flow_value(0, Type::I32, span),
                control_flow_value(1, Type::I32, span),
                control_flow_value(2, Type::I32, span),
            ],
            result: Type::I32,
            blocks: vec![
                Block {
                    id: BlockId(0),
                    parameters: Vec::new(),
                    instructions: Vec::new(),
                    terminators: control_flow_terminator(
                        span,
                        T::Jump {
                            target: BlockId(1),
                            arguments: vec![ValueId(0), ValueId(1), ValueId(2)],
                        },
                    ),
                },
                Block {
                    id: BlockId(1),
                    parameters: vec![
                        control_flow_value(3, Type::I32, span),
                        control_flow_value(4, Type::I32, span),
                        control_flow_value(5, Type::I32, span),
                    ],
                    instructions: vec![
                        control_flow_instruction(6, Type::I32, span, I::I32Literal(1)),
                        control_flow_instruction(
                            7,
                            Type::Bool,
                            span,
                            I::I32LtS { lhs: ValueId(3), rhs: ValueId(6) },
                        ),
                    ],
                    terminators: control_flow_terminator(
                        span,
                        T::Branch {
                            condition: ValueId(7),
                            true_target: BlockId(2),
                            true_arguments: Vec::new(),
                            false_target: BlockId(3),
                            false_arguments: vec![ValueId(4)],
                        },
                    ),
                },
                Block {
                    id: BlockId(2),
                    parameters: Vec::new(),
                    instructions: vec![
                        control_flow_instruction(8, Type::I32, span, I::I32Literal(1)),
                        control_flow_instruction(
                            9,
                            Type::I32,
                            span,
                            I::I32Add { lhs: ValueId(3), rhs: ValueId(8) },
                        ),
                    ],
                    terminators: control_flow_terminator(
                        span,
                        T::Jump {
                            target: BlockId(1),
                            arguments: vec![ValueId(9), ValueId(5), ValueId(4)],
                        },
                    ),
                },
                Block {
                    id: BlockId(3),
                    parameters: vec![control_flow_value(10, Type::I32, span)],
                    instructions: Vec::new(),
                    terminators: control_flow_terminator(span, T::Return(ValueId(10))),
                },
            ],
        };
        let boolean_identity = Function {
            id: FunctionId { module: ModuleId(0), declaration: 3 },
            entry_export: Some("Number".to_owned()),
            span,
            parameters: vec![control_flow_value(0, Type::Bool, span)],
            result: Type::Bool,
            blocks: vec![Block {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminators: control_flow_terminator(span, T::Return(ValueId(0))),
            }],
        };
        let program = control_flow_raw::Program {
            entry_module: ModuleId(0),
            modules: vec![Module {
                id: ModuleId(0),
                source_file: file,
                functions: vec![binary, operations, swap_loop, boolean_identity],
            }],
        };
        let verified = control_flow_v1::verify(program, &sources, file)
            .expect("complete ControlFlowV1 fixture must verify");
        (verified, sources)
    }

    #[test]
    fn emits_wrapping_i32_addition() {
        let sources = SourceMap::build(vec![SourceFileInput {
            path: "src/add.zry".to_owned(),
            text: "x".to_owned(),
        }])
        .expect("fixture source map must be valid");
        let path = NormalizedSourcePath::new("src/add.zry").expect("fixture path must be valid");
        let file = sources.file_id(&path).expect("fixture file must exist");
        let span = sources.span(file, 0, 1).expect("fixture span must be valid");
        let program = Program {
            functions: vec![Function {
                name: "add".to_owned(),
                parameters: vec![Type::I32, Type::I32],
                return_type: Type::I32,
                expressions: vec![
                    Expr { ty: Type::I32, span, kind: ExprKind::Parameter(0) },
                    Expr { ty: Type::I32, span, kind: ExprKind::Parameter(1) },
                    Expr {
                        ty: Type::I32,
                        span,
                        kind: ExprKind::I32Add { lhs: ExprId(0), rhs: ExprId(1) },
                    },
                ],
                body: ExprId(2),
            }],
        };
        let Ok(verified) = verify(program, &sources) else {
            panic!("test IR must be valid");
        };
        let Ok(artifact) = super::emit(&verified) else {
            panic!("JavaScript emission must succeed");
        };
        assert_eq!(
            artifact.source,
            concat!(
                "function $zryna$checkArity($zryna$actual, $zryna$expected) {\n",
                "  if ($zryna$actual !== $zryna$expected) {\n",
                "    throw new TypeError(\"ZRYNA-B2102: scalar ABI arity mismatch\");\n",
                "  }\n",
                "}\n",
                "\n",
                "function $zryna$i32($zryna$value) {\n",
                "  if (typeof $zryna$value !== \"number\") {\n",
                "    throw new TypeError(\"ZRYNA-B2001: expected a primitive JavaScript Number\");\n",
                "  }\n",
                "  if ($zryna$value !== ($zryna$value | 0) || ($zryna$value === 0 && 1 / $zryna$value < 0)) {\n",
                "    throw new RangeError(\"ZRYNA-B2002: expected a canonical signed 32-bit integer\");\n",
                "  }\n",
                "  return $zryna$value;\n",
                "}\n",
                "\n",
                "function $zryna$bool($zryna$value) {\n",
                "  if (typeof $zryna$value !== \"boolean\") {\n",
                "    throw new TypeError(\"ZRYNA-B2001: expected a primitive JavaScript Boolean\");\n",
                "  }\n",
                "  return $zryna$value;\n",
                "}\n",
                "\n",
                "function $zryna$imul($zryna$left, $zryna$right) {\n",
                "  const $zryna$leftLow = $zryna$left & 65535;\n",
                "  const $zryna$leftHigh = $zryna$left >>> 16;\n",
                "  const $zryna$rightLow = $zryna$right & 65535;\n",
                "  const $zryna$rightHigh = $zryna$right >>> 16;\n",
                "  return ($zryna$leftLow * $zryna$rightLow + ((($zryna$leftHigh * $zryna$rightLow + $zryna$leftLow * $zryna$rightHigh) & 65535) << 16)) | 0;\n",
                "}\n",
                "\n",
                "export function add(p0, p1) {\n",
                "  $zryna$checkArity(arguments.length, 2);\n",
                "  p0 = $zryna$i32(p0);\n",
                "  p1 = $zryna$i32(p1);\n",
                "  const v0 = p0;\n",
                "  const v1 = p1;\n",
                "  const v2 = (v0 + v1) | 0;\n",
                "  return $zryna$i32(v2);\n",
                "}\n",
            )
        );
    }

    #[test]
    fn emits_empty_program_as_deterministic_esm() {
        let sources = SourceMap::build(Vec::new()).expect("empty source map must be valid");
        let verified = verify(Program::default(), &sources).expect("empty IR must verify");
        let first = super::emit(&verified).expect("empty JavaScript module must emit");
        let second = super::emit(&verified).expect("repeated empty module must emit");

        assert_eq!(first, second);
        assert_eq!(first.source, "export {};\n");
    }

    #[test]
    fn emission_is_byte_deterministic_and_uses_collision_proof_private_names() {
        let sources = SourceMap::build(vec![SourceFileInput {
            path: "src/names.zry".to_owned(),
            text: "x".to_owned(),
        }])
        .expect("fixture source map must be valid");
        let path = NormalizedSourcePath::new("src/names.zry").expect("fixture path must be valid");
        let file = sources.file_id(&path).expect("fixture file must exist");
        let span = sources.span(file, 0, 1).expect("fixture span must be valid");
        let function = |name: &str, value: i32| Function {
            name: name.to_owned(),
            parameters: Vec::new(),
            return_type: Type::I32,
            expressions: vec![Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(value) }],
            body: ExprId(0),
        };
        let verified = verify(
            Program { functions: vec![function("zryna_i32", i32::MIN), function("_x", i32::MAX)] },
            &sources,
        )
        .expect("name fixture must verify");

        let first = super::emit(&verified).expect("name fixture must emit");
        let second = super::emit(&verified).expect("repeated name fixture must emit");

        assert_eq!(first, second);
        assert!(!first.source.contains('\r'));
        assert!(first.source.ends_with('\n'));
        assert_eq!(first.source.matches("function $zryna$i32").count(), 1);
        assert_eq!(first.source.matches("function $zryna$bool").count(), 1);
        assert!(first.source.contains("export function zryna_i32()"));
        assert!(first.source.contains("export function _x()"));
        assert!(
            first.source.find("zryna_i32()").expect("first export")
                < first.source.find("_x()").expect("second export")
        );
        assert!(first.source.contains("const v0 = -2147483648;"));
        assert!(first.source.contains("const v0 = 2147483647;"));
    }

    #[test]
    fn bool_remains_outside_the_verified_backend_profile() {
        let sources = SourceMap::build(vec![SourceFileInput {
            path: "src/bool.zry".to_owned(),
            text: "x".to_owned(),
        }])
        .expect("fixture source map must be valid");
        let path = NormalizedSourcePath::new("src/bool.zry").expect("fixture path must be valid");
        let file = sources.file_id(&path).expect("fixture file must exist");
        let span = sources.span(file, 0, 1).expect("fixture span must be valid");
        let program = Program {
            functions: vec![Function {
                name: "flag".to_owned(),
                parameters: Vec::new(),
                return_type: Type::Bool,
                expressions: vec![Expr { ty: Type::Bool, span, kind: ExprKind::BoolLiteral(true) }],
                body: ExprId(0),
            }],
        };

        assert!(verify(program, &sources).is_err());
    }

    #[test]
    fn emits_the_max_depth_tree_iteratively_and_linearly() {
        let sources = SourceMap::build(vec![SourceFileInput {
            path: "src/deep.zry".to_owned(),
            text: "x".to_owned(),
        }])
        .expect("fixture source map must be valid");
        let path = NormalizedSourcePath::new("src/deep.zry").expect("fixture path must be valid");
        let file = sources.file_id(&path).expect("fixture file must exist");
        let span = sources.span(file, 0, 1).expect("fixture span must be valid");
        let mut expressions = vec![Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(1) }];
        let mut root = ExprId(0);
        for _ in 1..MAX_IR_EXPRESSION_DEPTH {
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
        let expression_count = expressions.len();
        let program = Program {
            functions: vec![Function {
                name: "deepValue".to_owned(),
                parameters: Vec::new(),
                return_type: Type::I32,
                expressions,
                body: root,
            }],
        };
        let verified = verify(program, &sources).expect("maximum-depth IR must verify");
        let artifact = super::emit(&verified).expect("maximum-depth JavaScript must emit");
        assert_eq!(artifact.source.matches("  const v").count(), expression_count);
        assert!(artifact.source.len() < expression_count * 64);
    }

    #[test]
    fn emits_every_control_flow_operation_and_terminator_deterministically() {
        let (verified, _sources) = control_flow_fixture();
        let first = super::emit_control_flow(&verified).expect("M2 JavaScript must emit");
        let second = super::emit_control_flow(&verified).expect("repeated M2 JavaScript must emit");

        assert_eq!(first, second);
        assert!(!first.source.contains('\r'));
        assert!(first.source.ends_with('\n'));
        for expected in [
            "v3 = true;",
            "v4 = -2147483648;",
            "v5 = (v1 + v2) | 0;",
            "v6 = (v1 - v2) | 0;",
            "v7 = $zryna$imul(v1, v2);",
            "v8 = (0 - v4) | 0;",
            "v9 = v0 === v3;",
            "v10 = v1 !== v2;",
            "v11 = v1 < v2;",
            "v12 = v1 <= v2;",
            "v13 = v1 > v2;",
            "v14 = v1 >= v2;",
            "v15 = $zryna$m0f0(v1, v2);",
            "if (v0 === true)",
            "$zryna$block = 1;",
            "continue;",
            "return v16;",
        ] {
            assert!(first.source.contains(expected), "missing lowering: {expected}");
        }
        assert!(first.source.contains("export { $zryna$e0 as Math };"));
        assert!(first.source.contains("export { $zryna$e1 as Object };"));
        assert!(first.source.contains("export { $zryna$e2 as Number };"));
        assert!(!first.source.contains("export function"));
        assert!(!first.source.contains("eval("));
        assert!(!first.source.contains("import("));
        assert!(!first.source.contains("require("));
        assert!(!first.source.contains("process."));
    }

    #[test]
    fn emits_exact_multiplication_bool_branch_and_parallel_loop_edge() {
        let (verified, _sources) = control_flow_fixture();
        let artifact = super::emit_control_flow(&verified).expect("M2 JavaScript must emit");
        assert!(artifact.source.contains("v7 = $zryna$imul(v1, v2);"));
        assert!(artifact.source.contains("if (v0 === true)"));
        assert!(artifact.source.contains("$zryna$a1 = v5;"));
        assert!(artifact.source.contains("v4 = $zryna$a1;"));
        assert!(artifact.source.contains("return $zryna$bool($zryna$m0f3(p0));"));
    }

    #[test]
    fn emits_cross_module_calls_without_exporting_dependency_functions() {
        use control_flow_raw::{
            Block, BlockId, Function, FunctionId, InstructionKind as I, Module, ModuleId,
            Terminator as T, ValueId,
        };

        let sources = SourceMap::build(vec![
            SourceFileInput { path: "src/dep.zry".to_owned(), text: "x".to_owned() },
            SourceFileInput { path: "src/main.zry".to_owned(), text: "x".to_owned() },
        ])
        .expect("module fixture source map");
        let dep_path = NormalizedSourcePath::new("src/dep.zry").expect("dep path");
        let main_path = NormalizedSourcePath::new("src/main.zry").expect("main path");
        let dep_file = sources.file_id(&dep_path).expect("dep file");
        let main_file = sources.file_id(&main_path).expect("main file");
        let dep_span = sources.span(dep_file, 0, 1).expect("dep span");
        let main_span = sources.span(main_file, 0, 1).expect("main span");
        let dep = Function {
            id: FunctionId { module: ModuleId(0), declaration: 0 },
            entry_export: None,
            span: dep_span,
            parameters: Vec::new(),
            result: Type::I32,
            blocks: vec![Block {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![control_flow_instruction(
                    0,
                    Type::I32,
                    dep_span,
                    I::I32Literal(41),
                )],
                terminators: control_flow_terminator(dep_span, T::Return(ValueId(0))),
            }],
        };
        let main = Function {
            id: FunctionId { module: ModuleId(1), declaration: 0 },
            entry_export: Some("Number".to_owned()),
            span: main_span,
            parameters: Vec::new(),
            result: Type::I32,
            blocks: vec![Block {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![
                    control_flow_instruction(
                        0,
                        Type::I32,
                        main_span,
                        I::DirectCall {
                            callee: FunctionId { module: ModuleId(0), declaration: 0 },
                            arguments: Vec::new(),
                        },
                    ),
                    control_flow_instruction(1, Type::I32, main_span, I::I32Literal(1)),
                    control_flow_instruction(
                        2,
                        Type::I32,
                        main_span,
                        I::I32Add { lhs: ValueId(0), rhs: ValueId(1) },
                    ),
                ],
                terminators: control_flow_terminator(main_span, T::Return(ValueId(2))),
            }],
        };
        let verified = control_flow_v1::verify(
            control_flow_raw::Program {
                entry_module: ModuleId(1),
                modules: vec![
                    Module { id: ModuleId(0), source_file: dep_file, functions: vec![dep] },
                    Module { id: ModuleId(1), source_file: main_file, functions: vec![main] },
                ],
            },
            &sources,
            main_file,
        )
        .expect("cross-module fixture must verify");
        let artifact = super::emit_control_flow(&verified).expect("cross-module ESM must emit");
        assert!(artifact.source.contains("v0 = $zryna$m0f0();"));
        assert!(artifact.source.contains("export { $zryna$e0 as Number };"));
        assert_eq!(artifact.source.matches("export {").count(), 1);
    }

    #[test]
    fn control_flow_artifact_budget_fails_at_the_first_extra_byte() {
        let (verified, _sources) = control_flow_fixture();
        let artifact = super::emit_control_flow(&verified).expect("fixture must emit");
        let exact = super::emit_control_flow_with_budget(&verified, artifact.source.len())
            .expect("exact rendered byte budget must pass");
        assert_eq!(artifact, exact);
        let diagnostic = super::emit_control_flow_with_budget(&verified, artifact.source.len() - 1)
            .expect_err("one byte below the exact rendered artifact must fail");
        assert_eq!(diagnostic.code(), "ZRYNA-J2003");
    }
}
