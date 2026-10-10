use super::{invalid, malformed};
use wasmparser::{BlockType, FunctionBody, Operator, ValType};
use zryna_diagnostics::Diagnostic;

const REFERENCES: [&str; 9] = [
    include_str!("shape/language-allocate.txt"),
    include_str!("shape/copy.txt"),
    include_str!("shape/realloc.txt"),
    include_str!("shape/validate.txt"),
    include_str!("shape/drain.txt"),
    include_str!("shape/find.txt"),
    include_str!("shape/allocate.txt"),
    include_str!("shape/free.txt"),
    include_str!("shape/state.txt"),
];

pub(super) fn audit(body: &FunctionBody<'_>, role: usize) -> Result<(), Diagnostic> {
    let mut expected = REFERENCES[role]
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    let mut operators = body.get_operators_reader().map_err(malformed)?;
    while !operators.eof() {
        let actual = normalized(&operators.read().map_err(malformed)?)?;
        if expected.next() != Some(actual.as_str()) {
            return Err(invalid(
                "storage helper differs from its independent complete instruction reference",
            ));
        }
    }
    if expected.next().is_some() {
        return Err(invalid("storage helper omits an independently required instruction"));
    }
    Ok(())
}

fn normalized(operator: &Operator<'_>) -> Result<String, Diagnostic> {
    let text = match operator {
        Operator::LocalGet { local_index } => format!("local.get {local_index}"),
        Operator::LocalSet { local_index } => format!("local.set {local_index}"),
        Operator::LocalTee { local_index } => format!("local.tee {local_index}"),
        Operator::GlobalGet { global_index } => format!("global.get {global_index}"),
        Operator::GlobalSet { global_index } => format!("global.set {global_index}"),
        Operator::I32Const { value } => format!("i32.const {value}"),
        Operator::Call { function_index } => format!("call {function_index}"),
        Operator::Br { relative_depth } => format!("br {relative_depth}"),
        Operator::BrIf { relative_depth } => format!("br_if {relative_depth}"),
        Operator::I32Load { memarg } => format!("i32.load {}", memarg.offset),
        Operator::I32Store { memarg } => format!("i32.store {}", memarg.offset),
        Operator::I32Load8U { .. } => "i32.load8_u".to_owned(),
        Operator::I32Store8 { .. } => "i32.store8".to_owned(),
        Operator::If { blockty: BlockType::Type(ValType::I32) } => "if i32".to_owned(),
        Operator::If { blockty: BlockType::Empty } => "if".to_owned(),
        Operator::Block { blockty: BlockType::Empty } => "block".to_owned(),
        Operator::Loop { blockty: BlockType::Empty } => "loop".to_owned(),
        Operator::Unreachable => "unreachable".to_owned(),
        Operator::Else => "else".to_owned(),
        Operator::End => "end".to_owned(),
        Operator::Return => "return".to_owned(),
        Operator::I32Eqz => "i32.eqz".to_owned(),
        Operator::I32Eq => "i32.eq".to_owned(),
        Operator::I32Ne => "i32.ne".to_owned(),
        Operator::I32LtU => "i32.lt_u".to_owned(),
        Operator::I32GtU => "i32.gt_u".to_owned(),
        Operator::I32GeU => "i32.ge_u".to_owned(),
        Operator::I32Add => "i32.add".to_owned(),
        Operator::I32Sub => "i32.sub".to_owned(),
        Operator::I32And => "i32.and".to_owned(),
        _ => return Err(invalid("storage reference contains an unsupported instruction shape")),
    };
    Ok(text)
}
