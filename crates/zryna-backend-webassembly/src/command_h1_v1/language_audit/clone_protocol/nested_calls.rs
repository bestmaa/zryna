//! Independently authored child operations use only sealed layout facts.

use std::collections::BTreeMap;
use wasmparser::{BlockType, FunctionBody, Operator, ValType};
use zryna_diagnostics::Diagnostic;
use zryna_ir::command_h1_v1::VerifiedProgram;
use zryna_layout::{TypeCategory, TypeId, VerifiedLayouts, VerifiedType};

use super::super::{declarations::Shape, invalid};

mod clone_suffix;
mod drop_body;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Token {
    Constant(i32),
    Local(u32),
    SetLocal(u32),
    Tee(u32),
    Global(u32),
    SetGlobal(u32),
    Call(u32),
    Add,
    Subtract,
    Multiply,
    Equal,
    Zero,
    GreaterEqual,
    Load,
    Store,
    StoreByte,
    Block,
    Loop,
    If,
    End,
    Branch(u32),
    BranchIf(u32),
    Return,
    Trap,
    Other,
}

struct Reference {
    disabled: Vec<Token>,
    enabled: Vec<Token>,
    suffix: bool,
    locals: u32,
}

pub(super) struct References {
    bodies: BTreeMap<u32, Reference>,
}

impl References {
    pub(super) fn derive(program: &VerifiedProgram, shape: &Shape) -> Result<Self, Diagnostic> {
        let layouts = program.linear32_layouts();
        let mut bodies = BTreeMap::new();
        for ty in layouts.types() {
            let composite = matches!(
                ty.category(),
                TypeCategory::Struct
                    | TypeCategory::Enum
                    | TypeCategory::FixedArray
                    | TypeCategory::Vec
            );
            let disabled = if composite {
                clone_suffix::derive(ty, layouts, shape, false)?
            } else {
                Vec::new()
            };
            let enabled = if composite {
                clone_suffix::derive(ty, layouts, shape, true)?
            } else {
                Vec::new()
            };
            bodies.insert(
                6 + ty.id().index(),
                Reference { disabled, enabled, suffix: true, locals: 5 },
            );
            let dropped = drop_body::derive(ty, layouts, shape)?;
            bodies.insert(
                6 + shape.type_count + ty.id().index(),
                Reference { disabled: dropped.clone(), enabled: dropped, suffix: false, locals: 3 },
            );
        }
        Ok(Self { bodies })
    }

    pub(super) fn audit(
        &self,
        body: &FunctionBody<'_>,
        index: u32,
        enabled: bool,
        shape: &Shape,
    ) -> Result<(), Diagnostic> {
        let reference = self.bodies.get(&index).ok_or_else(invalid)?;
        let mut locals = body.get_locals_reader().map_err(|_| invalid())?;
        if locals.get_count() != 1
            || locals.read().map_err(|_| invalid())? != (reference.locals, ValType::I32)
        {
            return Err(invalid());
        }
        let actual = body
            .get_operators_reader()
            .map_err(|_| invalid())?
            .into_iter()
            .map(|operator| operator.map(|operator| token(&operator)).map_err(|_| invalid()))
            .collect::<Result<Vec<_>, _>>()?;
        let expected = if enabled { &reference.enabled } else { &reference.disabled };
        let start = if reference.suffix {
            actual.len().checked_sub(expected.len()).ok_or_else(invalid)?
        } else {
            0
        };
        if actual.get(start..) != Some(expected.as_slice()) {
            return Err(invalid());
        }
        // Scalar clone helpers have no child operations. Composite clone suffixes
        // include every child clone and every reversed prefix drop; none may precede them.
        if actual[..start].iter().any(|token| matches!(token, Token::Call(callee) if *callee >= 6 && *callee < 6 + 2 * shape.type_count)) {
            return Err(invalid());
        }
        Ok(())
    }
}

fn token(operator: &Operator<'_>) -> Token {
    use Token as T;
    match operator {
        Operator::I32Const { value } => T::Constant(*value),
        Operator::LocalGet { local_index } => T::Local(*local_index),
        Operator::LocalSet { local_index } => T::SetLocal(*local_index),
        Operator::LocalTee { local_index } => T::Tee(*local_index),
        Operator::GlobalGet { global_index } => T::Global(*global_index),
        Operator::GlobalSet { global_index } => T::SetGlobal(*global_index),
        Operator::Call { function_index } => T::Call(*function_index),
        Operator::I32Add => T::Add,
        Operator::I32Sub => T::Subtract,
        Operator::I32Mul => T::Multiply,
        Operator::I32Eq => T::Equal,
        Operator::I32Eqz => T::Zero,
        Operator::I32GeU => T::GreaterEqual,
        Operator::I32Load { memarg }
            if memarg.align == 2 && memarg.offset == 0 && memarg.memory == 0 =>
        {
            T::Load
        }
        Operator::I32Store { memarg }
            if memarg.align == 2 && memarg.offset == 0 && memarg.memory == 0 =>
        {
            T::Store
        }
        Operator::I32Store8 { memarg }
            if memarg.align == 0 && memarg.offset == 0 && memarg.memory == 0 =>
        {
            T::StoreByte
        }
        Operator::Block { blockty: BlockType::Empty } => T::Block,
        Operator::Loop { blockty: BlockType::Empty } => T::Loop,
        Operator::If { blockty: BlockType::Empty } => T::If,
        Operator::End => T::End,
        Operator::Br { relative_depth } => T::Branch(*relative_depth),
        Operator::BrIf { relative_depth } => T::BranchIf(*relative_depth),
        Operator::Return => T::Return,
        Operator::Unreachable => T::Trap,
        _ => T::Other,
    }
}

fn constant(value: u64) -> Result<Token, Diagnostic> {
    i32::try_from(value).map(Token::Constant).map_err(|_| invalid())
}

fn address(out: &mut Vec<Token>, local: u32, offset: u64) -> Result<(), Diagnostic> {
    out.push(Token::Local(local));
    if offset != 0 {
        out.extend([constant(offset)?, Token::Add]);
    }
    Ok(())
}

fn load(out: &mut Vec<Token>, ty: VerifiedType<'_>) {
    if !matches!(
        ty.category(),
        TypeCategory::Struct | TypeCategory::Enum | TypeCategory::FixedArray
    ) {
        // Child operations are restricted to owned values, so Bool is never loaded.
        out.push(Token::Load);
    }
}

fn store(out: &mut Vec<Token>, ty: VerifiedType<'_>) -> Result<(), Diagnostic> {
    match ty.category() {
        TypeCategory::Struct | TypeCategory::Enum | TypeCategory::FixedArray => {
            out.extend([constant(ty.size())?, Token::Call(1)]);
        }
        TypeCategory::Bool => out.push(Token::StoreByte),
        _ => out.push(Token::Store),
    }
    Ok(())
}

fn child(layouts: &VerifiedLayouts, id: TypeId) -> Result<VerifiedType<'_>, Diagnostic> {
    layouts.type_by_id(id).ok_or_else(invalid)
}

fn drop_child(
    out: &mut Vec<Token>,
    ty: VerifiedType<'_>,
    offset: u64,
    shape: &Shape,
) -> Result<(), Diagnostic> {
    if ty.drop_kind() != 0 {
        address(out, 0, offset)?;
        load(out, ty);
        out.push(Token::Call(6 + shape.type_count + ty.id().index()));
    }
    Ok(())
}

fn record(out: &mut Vec<Token>, kind: i32, shape: &Shape) {
    out.extend([Token::Constant(0x1000_0000 + kind), Token::Call(shape.run + 2)]);
}

fn failed(out: &mut Vec<Token>, enabled: bool, shape: &Shape) {
    use Token as T;
    if enabled {
        out.extend([T::Global(9), T::Constant(2), T::Equal, T::If]);
        for label in [6, 7, 8] {
            out.extend([T::Global(label), T::Call(shape.run + 2)]);
        }
        out.extend([T::End, T::Constant(0), T::SetGlobal(9)]);
    }
}

fn indexed(
    out: &mut Vec<Token>,
    base: u32,
    index: u32,
    stride: u64,
    header: bool,
) -> Result<(), Diagnostic> {
    address(out, base, if header { 12 } else { 0 })?;
    out.extend([Token::Local(index), constant(stride)?, Token::Multiply, Token::Add]);
    Ok(())
}
