//! Deterministic JavaScript for the separately sealed immutable generic Copy lane.

use crate::JavaScriptArtifact;
use std::fmt::Write;
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::{copy_v1::VerifiedCopyProgram, raw};

#[cfg(test)]
mod tests;

/// Emits only a complete wire/source/runtime-bound Copy program.
///
/// ```compile_fail
/// fn raw(program: &zryna_ir::generic_v1::raw::Program) {
///     zryna_backend_javascript::generic_copy_v1::emit(program);
/// }
/// ```
///
/// # Errors
/// Rejects output exhaustion or any unexpected invariant of the retained verified program.
pub fn emit(program: &VerifiedCopyProgram<'_>) -> Result<JavaScriptArtifact, Diagnostic> {
    let mut writer = Writer { source: String::new() };
    writer.text(crate::prelude::JAVASCRIPT_PRELUDE)?;
    for (index, function) in program.functions().iter().enumerate() {
        writer.line(format_args!(
            "function $g{index}({}) {{",
            parameters(function.parameters.len())
        ))?;
        let count = function
            .blocks
            .iter()
            .map(|block| block.parameters.len() + block.instructions.len())
            .sum::<usize>();
        for id in 0..count {
            writer.line(format_args!("let $v{id};"))?;
        }
        for (index, parameter) in function.blocks[0].parameters.iter().enumerate() {
            writer.line(format_args!("$v{} = $a{index};", parameter.id))?;
        }
        writer.text("let $block = 0;\nwhile (true) {\nswitch ($block) {\n")?;
        for block in &function.blocks {
            writer.line(format_args!("case {}: {{", block.id))?;
            for instruction in &block.instructions {
                let expression = operation(program, &instruction.operation)?;
                writer.line(format_args!("$v{} = {expression};", instruction.result.id))?;
            }
            terminator(&mut writer, function, &block.terminator)?;
            writer.text("}\n")?;
        }
        writer
            .text("default: throw new Error('ZRYNA-I7001: invalid private block');\n}\n}\n}\n")?;
    }
    for (export, function) in program.scalar_abi().exports().zip(program.export_functions()) {
        let index = export.index();
        writer.line(format_args!(
            "function $export{index}({}) {{",
            parameters(export.parameters().len())
        ))?;
        writer.line(format_args!(
            "$zryna$checkArity(arguments.length, {});",
            export.parameters().len()
        ))?;
        for (argument, _) in export.parameters().iter().enumerate() {
            let check = scalar_check(program.functions()[*function].parameters[argument])?;
            writer.line(format_args!("$a{argument} = {check}($a{argument});"))?;
        }
        writer.line(format_args!(
            "return {}($g{function}({}));",
            scalar_check(program.functions()[*function].result)?,
            parameters(export.parameters().len())
        ))?;
        writer.line(format_args!(
            "}}\nexport {{ $export{index} as {} }};",
            export.javascript_name().as_str()
        ))?;
    }
    if program.scalar_abi().exports().len() == 0 {
        writer.text("export {};\n")?;
    }
    Ok(JavaScriptArtifact { source: writer.source })
}

fn scalar_check(ty: raw::Type) -> Result<&'static str, Diagnostic> {
    match ty {
        raw::Type::Stored(0) => Ok("$zryna$bool"),
        raw::Type::Stored(1) => Ok("$zryna$i32"),
        _ => Err(internal()),
    }
}

fn parameters(count: usize) -> String {
    (0..count).map(|id| format!("$a{id}")).collect::<Vec<_>>().join(", ")
}
fn operands(ids: &[u32]) -> String {
    ids.iter().map(|id| format!("$v{id}")).collect::<Vec<_>>().join(", ")
}

fn operation(
    program: &VerifiedCopyProgram<'_>,
    operation: &raw::Operation,
) -> Result<String, Diagnostic> {
    Ok(match operation {
        raw::Operation::BoolLiteral(value) => value.to_string(),
        raw::Operation::I32Literal(value) => value.to_string(),
        raw::Operation::Unit => "undefined".into(),
        raw::Operation::Copy { value } => format!("$v{value}"),
        raw::Operation::I32Add { left, right } => format!("(($v{left} + $v{right}) | 0)"),
        raw::Operation::ClosedGenericCall { instance, arguments } => {
            format!("$g{instance}({})", operands(arguments))
        }
        raw::Operation::SourceCall { module, function, arguments } => {
            let mut key = [0u8; 9];
            key[0] = 0x41;
            key[1..5].copy_from_slice(&module.to_le_bytes());
            key[5..9].copy_from_slice(&function.to_le_bytes());
            let index = program
                .functions()
                .binary_search_by(|candidate| candidate.key.as_slice().cmp(&key))
                .map_err(|_| internal())?;
            format!("$g{index}({})", operands(arguments))
        }
        raw::Operation::ClosedEnumConstruct { ordinal, payload, .. } => {
            let payload = payload.map_or_else(|| "undefined".into(), |value| format!("$v{value}"));
            format!("{{ tag: {ordinal}, payload: {payload} }}")
        }
    })
}

fn edge(
    writer: &mut Writer,
    function: &raw::Function,
    edge: &raw::Edge,
    payload: Option<u32>,
) -> Result<(), Diagnostic> {
    let block = function.blocks.get(edge.target as usize).ok_or_else(internal)?;
    writer.line(format_args!("const $edge = [{}];", operands(&edge.arguments)))?;
    let offset = usize::from(payload.is_some());
    if let Some(payload) = payload {
        writer.line(format_args!("$v{} = $v{payload}.payload;", block.parameters[0].id))?;
    }
    for (index, parameter) in block.parameters.iter().skip(offset).enumerate() {
        writer.line(format_args!("$v{} = $edge[{index}];", parameter.id))?;
    }
    writer.line(format_args!("$block = {};\ncontinue;", edge.target))
}

fn terminator(
    writer: &mut Writer,
    function: &raw::Function,
    terminator: &raw::Terminator,
) -> Result<(), Diagnostic> {
    match terminator {
        raw::Terminator::Return(value) => writer.line(format_args!("return $v{value};")),
        raw::Terminator::Jump(jump) => edge(writer, function, jump, None),
        raw::Terminator::Branch { condition, yes, no } => {
            writer.line(format_args!("if ($v{condition}) {{"))?;
            edge(writer, function, yes, None)?;
            writer.text("} else {\n")?;
            edge(writer, function, no, None)?;
            writer.text("}\n")
        }
        raw::Terminator::ClosedEnumMatch {
            scrutinee, mode: raw::MatchMode::Value, arms, ..
        } => {
            writer.line(format_args!("switch ($v{scrutinee}.tag) {{"))?;
            for arm in arms {
                writer.line(format_args!("case {}: {{", arm.ordinal))?;
                edge(writer, function, &arm.edge, arm.binding.as_ref().map(|_| *scrutinee))?;
                writer.text("}\n")?;
            }
            writer
                .text("default: throw new Error('ZRYNA-I7001: invalid private discriminant');\n}\n")
        }
        raw::Terminator::ClosedEnumMatch { .. } => Err(internal()),
    }
}

struct Writer {
    source: String,
}
impl Writer {
    fn text(&mut self, text: &str) -> Result<(), Diagnostic> {
        if self
            .source
            .len()
            .checked_add(text.len())
            .is_none_or(|length| length > crate::prelude::MAX_CONTROL_FLOW_JAVASCRIPT_BYTES)
        {
            return Err(emission_budget());
        }
        self.source.try_reserve(text.len()).map_err(|_| emission_budget())?;
        self.source.push_str(text);
        Ok(())
    }
    fn line(&mut self, args: std::fmt::Arguments<'_>) -> Result<(), Diagnostic> {
        let mut text = String::new();
        text.write_fmt(args).map_err(|_| internal())?;
        self.text(&text)?;
        self.text("\n")
    }
}
fn internal() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-I7001",
        None,
        "generic Copy JavaScript emission invariant failed",
        "retain the complete sealed successor program",
    )
}

fn emission_budget() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-J2003",
        None,
        "generic Copy JavaScript artifact exceeds its emission resource budget",
        "reduce the complete sealed successor program below the JavaScript artifact budget",
    )
}
