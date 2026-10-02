use wasmparser::{BlockType, Operator};
use zryna_diagnostics::Diagnostic;

use super::invalid;

pub(super) fn operator(operator: &Operator<'_>) -> Result<String, Diagnostic> {
    Ok(match operator {
        Operator::LocalGet { local_index } => format!("local.get {local_index}"),
        Operator::LocalSet { local_index } => format!("local.set {local_index}"),
        Operator::LocalTee { local_index } => format!("local.tee {local_index}"),
        Operator::GlobalGet { global_index } => format!("global.get {global_index}"),
        Operator::I32Const { value } => format!("i32.const {value}"),
        Operator::Call { function_index } => format!("call {function_index}"),
        Operator::Br { relative_depth } => format!("br {relative_depth}"),
        Operator::BrIf { relative_depth } => format!("br_if {relative_depth}"),
        Operator::I32Load { memarg } => memory("i32.load", memarg),
        Operator::I32Store { memarg } => memory("i32.store", memarg),
        Operator::I32Load8U { memarg } => memory("i32.load8_u", memarg),
        Operator::If { blockty: BlockType::Empty } => "if".to_owned(),
        Operator::Block { blockty: BlockType::Empty } => "block".to_owned(),
        Operator::Loop { blockty: BlockType::Empty } => "loop".to_owned(),
        Operator::Unreachable => "unreachable".to_owned(),
        Operator::Drop => "drop".to_owned(),
        Operator::Else => "else".to_owned(),
        Operator::End => "end".to_owned(),
        Operator::Return => "return".to_owned(),
        Operator::I32Eqz => "i32.eqz".to_owned(),
        Operator::I32Eq => "i32.eq".to_owned(),
        Operator::I32Ne => "i32.ne".to_owned(),
        Operator::I32LtU => "i32.lt_u".to_owned(),
        Operator::I32GtU => "i32.gt_u".to_owned(),
        Operator::I32LeU => "i32.le_u".to_owned(),
        Operator::I32GeU => "i32.ge_u".to_owned(),
        Operator::I32Add => "i32.add".to_owned(),
        Operator::I32Sub => "i32.sub".to_owned(),
        Operator::I32And => "i32.and".to_owned(),
        Operator::I32Or => "i32.or".to_owned(),
        _ => return Err(invalid()),
    })
}

fn memory(name: &str, argument: &wasmparser::MemArg) -> String {
    format!("{name} memory={} align={} offset={}", argument.memory, argument.align, argument.offset)
}
