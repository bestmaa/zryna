use super::{Context, Locals, index_error, memory, observation, operations};
use wasm_encoder::{BlockType, Function, Instruction as I, MemArg};
use zryna_ir::data_ownership_v1::{VerifiedDropAction, VerifiedDropActionKind, VerifiedFunction};

mod plan;
use plan::{Kind, Plan, Segment};

const WORD: MemArg = MemArg { offset: 0, align: 2, memory_index: 0 };

pub(super) fn action(
    function: VerifiedFunction<'_>,
    action: &VerifiedDropAction,
    locals: Locals,
    context: &Context<'_>,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    // Dynamic clone frontiers are consumed inside the clone helper, before publication.
    if action.kind() != VerifiedDropActionKind::Place {
        return Err(index_error());
    }
    let plan = Plan::derive(function, action, context.layouts)?;
    observation::root(function, action.root().index(), context, body);
    Emitter { function, root: action.root().index(), locals, context, body }.node(&plan, 0, false)
}

struct Emitter<'a, 'b> {
    function: VerifiedFunction<'a>,
    root: u32,
    locals: Locals,
    context: &'b Context<'a>,
    body: &'b mut Function,
}

impl Emitter<'_, '_> {
    fn node(
        &mut self,
        plan: &Plan,
        offset: u64,
        projected: bool,
    ) -> Result<(), zryna_diagnostics::Diagnostic> {
        let ty = self.context.layouts.type_by_id(plan.ty).ok_or_else(index_error)?;
        if matches!(plan.kind, Kind::Complete) {
            self.address(offset)?;
            if projected {
                memory::load_value(ty, self.body);
            }
            self.body.instruction(&I::Call(self.context.drop_index(plan.ty)));
            return Ok(());
        }
        if let Some(kind) = observation::value_kind(ty.category()) {
            observation::record(0x1000_0000 + kind, self.context, self.body);
        }
        match &plan.kind {
            Kind::Struct(children) => {
                for (child_offset, child) in children.iter().rev() {
                    self.node(child, add(offset, *child_offset)?, true)?;
                }
            }
            Kind::Enum { active, payloads } => {
                for (ordinal, child_offset, child) in payloads {
                    if active.is_none() {
                        self.address(offset)?;
                        self.body.instruction(&I::I32Load(WORD));
                        self.body.instruction(&I::I32Const(
                            i32::try_from(*ordinal).map_err(|_| index_error())?,
                        ));
                        self.body.instruction(&I::I32Eq);
                        self.body.instruction(&I::If(BlockType::Empty));
                    }
                    self.node(child, add(offset, *child_offset)?, true)?;
                    if active.is_none() {
                        self.body.instruction(&I::End);
                    }
                }
            }
            Kind::Array(segments) => {
                let child = ty.referenced_type().ok_or_else(index_error)?;
                let stride = ty.array_stride().ok_or_else(index_error)?;
                for segment in segments.iter().rev() {
                    match segment {
                        Segment::Complete { start, end } => {
                            self.range(child, stride, offset, *start, *end)?;
                        }
                        Segment::Projected { index, plan } => self.node(
                            plan,
                            add(
                                offset,
                                stride.checked_mul(u64::from(*index)).ok_or_else(index_error)?,
                            )?,
                            true,
                        )?,
                    }
                }
            }
            Kind::Complete => unreachable!(),
        }
        Ok(())
    }

    fn range(
        &mut self,
        child: zryna_layout::TypeId,
        stride: u64,
        offset: u64,
        start: u32,
        end: u32,
    ) -> Result<(), zryna_diagnostics::Diagnostic> {
        let ty = self.context.layouts.type_by_id(child).ok_or_else(index_error)?;
        if ty.drop_kind() == 0 {
            return Ok(());
        }
        self.body.instruction(&I::I32Const(i32::try_from(end).map_err(|_| index_error())?));
        self.body.instruction(&I::LocalSet(self.locals.scratch));
        self.body.instruction(&I::Block(BlockType::Empty));
        self.body.instruction(&I::Loop(BlockType::Empty));
        self.body.instruction(&I::LocalGet(self.locals.scratch));
        self.body.instruction(&I::I32Const(i32::try_from(start).map_err(|_| index_error())?));
        self.body.instruction(&I::I32Eq);
        self.body.instruction(&I::BrIf(1));
        self.body.instruction(&I::LocalGet(self.locals.scratch));
        self.body.instruction(&I::I32Const(1));
        self.body.instruction(&I::I32Sub);
        self.body.instruction(&I::LocalSet(self.locals.scratch));
        self.address(offset)?;
        self.body.instruction(&I::LocalGet(self.locals.scratch));
        self.body.instruction(&I::I32Const(i32::try_from(stride).map_err(|_| index_error())?));
        self.body.instruction(&I::I32Mul);
        self.body.instruction(&I::I32Add);
        memory::load_value(ty, self.body);
        self.body.instruction(&I::Call(self.context.drop_index(child)));
        self.body.instruction(&I::Br(0));
        self.body.instruction(&I::End);
        self.body.instruction(&I::End);
        Ok(())
    }

    fn address(&mut self, offset: u64) -> Result<(), zryna_diagnostics::Diagnostic> {
        operations::place_value(
            self.function,
            self.root,
            self.locals,
            self.context.layouts,
            self.body,
        )?;
        if offset != 0 {
            self.body.instruction(&I::I32Const(i32::try_from(offset).map_err(|_| index_error())?));
            self.body.instruction(&I::I32Add);
        }
        Ok(())
    }
}

fn add(left: u64, right: u64) -> Result<u64, zryna_diagnostics::Diagnostic> {
    left.checked_add(right).ok_or_else(index_error)
}
