use super::super::expression_decisions::ExpressionKind;
use super::{
    Children, ConstructorKind, Frame, HandleFrame, HandleOperation, IndexedObservation, Leaf,
    PreparationContext, StringOperation, Ty, VisitOutcome,
};

impl<'f> PreparationContext<'_, 'f, '_, '_> {
    pub(super) fn visit(
        &mut self,
        id: u32,
        expected: Option<Ty>,
        frames: &mut Vec<Frame<'f>>,
    ) -> Option<VisitOutcome> {
        self.visits = self.visits.checked_add(1)?;
        if let IndexedObservation::Value(value) = self.structured_handoff(id, expected)? {
            return Some(VisitOutcome::Value(value));
        }
        if let IndexedObservation::Value(value) = self.prepared_observation(id, expected)? {
            return Some(VisitOutcome::Value(value));
        }
        let decision = self.decisions.classify_prepared(id, expected, self.state.summary)?;
        let at = decision.at;
        if let ExpressionKind::Scalar { operation, ref inputs } = decision.kind {
            frames.push(self.enter_scalar(operation, inputs.clone(), expected, decision.ty?, at));
            return Some(VisitOutcome::Deferred);
        }
        if let ExpressionKind::Call { callee, arguments } = decision.kind {
            frames.push(self.enter_call(callee, arguments, decision.ty, at)?);
            return Some(VisitOutcome::Deferred);
        }
        if let ExpressionKind::InferredClone(operand) = decision.kind {
            return self.inferred_clone(operand, at, frames);
        }
        let ty = match (&decision.kind, decision.ty) {
            (_, Some(ty)) => ty,
            (ExpressionKind::Reference(name), None) => self.inferred_reference_type(name)?,
            (ExpressionKind::Projection(id), None) => {
                let source = self.resolve(*id)?;
                let value = self.resolved_projection(source, source.ty, at)?;
                return Some(VisitOutcome::Value(value));
            }
            _ => {
                self.decisions.errors.at(
                    "ZRYNA-M3016",
                    at,
                    "expression requires an exact owned contextual type",
                    "use a supported typed scalar operand",
                );
                return None;
            }
        };
        self.visit_kind(decision.kind, ty, at, frames)
    }

    fn visit_kind(
        &mut self,
        kind: ExpressionKind<'f>,
        ty: Ty,
        at: zryna_source::Span,
        frames: &mut Vec<Frame<'f>>,
    ) -> Option<VisitOutcome> {
        let value = match kind {
            ExpressionKind::Scalar { .. } => unreachable!("scalar frame entered"),
            ExpressionKind::InferredClone(_) => unreachable!("inferred clone selected"),
            ExpressionKind::Bool(value) => self.emit_leaf(Leaf::Bool(value), ty, at),
            ExpressionKind::I32(value) => self.emit_leaf(Leaf::I32(value), ty, at),
            ExpressionKind::String(bytes) => {
                let cleanup = self.reverse(ty, at)?;
                self.emit_leaf(Leaf::String { bytes, cleanup }, ty, at)
            }
            ExpressionKind::Environment(key) => {
                let cleanup = self.reverse(ty, at)?;
                self.emit_leaf(Leaf::Environment { key, cleanup }, ty, at)
            }
            ExpressionKind::Reference(name) => self.reference(name, ty, at),
            ExpressionKind::Projection(id) => self.projection(id, ty, at),
            ExpressionKind::StringClone(id) => {
                if self.state.summary && self.compound_string_read(id)? {
                    frames.push(self.enter_string(StringOperation::Clone, vec![id], ty, at)?);
                    return Some(VisitOutcome::Deferred);
                }
                self.string_clone(id, ty, at)
            }
            ExpressionKind::StringConcat { arguments, callee } => {
                self.require_string_scope(at)?;
                let inputs = crate::data_ownership_v1::owned_string_read::concat_arguments(
                    arguments,
                    callee,
                    self.decisions.errors,
                )?;
                frames.push(self.enter_string(StringOperation::Concat, inputs.to_vec(), ty, at)?);
                return Some(VisitOutcome::Deferred);
            }
            ExpressionKind::AggregateClone(id) => self.aggregate_clone(id, ty, at),
            ExpressionKind::HandleClone(id) => {
                let operation = if ty.category == zryna_layout::TypeCategory::Shared {
                    HandleOperation::SharedClone
                } else {
                    HandleOperation::WeakClone
                };
                return self.visit_handle_read(id, ty, ty, operation, at, frames);
            }
            ExpressionKind::Shared(id) => {
                let payload = self.handle_payload(ty)?;
                frames.push(Frame::Handle(HandleFrame { result: ty, at }));
                frames.push(Frame::Visit(id, Some(payload)));
                return Some(VisitOutcome::Deferred);
            }
            ExpressionKind::Downgrade(id) => {
                let shared = self.shared_for_weak(ty)?;
                return self.visit_handle_read(
                    id,
                    shared,
                    ty,
                    HandleOperation::WeakDowngrade,
                    at,
                    frames,
                );
            }
            ExpressionKind::Call { .. } => unreachable!("call frame entered"),
            ExpressionKind::Struct(decision) => {
                frames.push(self.enter(
                    Children::Struct(decision),
                    ty,
                    at,
                    ConstructorKind::Struct,
                )?);
                return Some(VisitOutcome::Deferred);
            }
            ExpressionKind::Array(decision) => {
                frames.push(self.enter(
                    Children::Array(decision),
                    ty,
                    at,
                    ConstructorKind::FixedArray,
                )?);
                return Some(VisitOutcome::Deferred);
            }
            ExpressionKind::Vec(decision) => {
                frames.push(self.enter(Children::Array(decision), ty, at, ConstructorKind::Vec)?);
                return Some(VisitOutcome::Deferred);
            }
            ExpressionKind::Enum(decision) => {
                let kind = ConstructorKind::Enum { variant: u32::try_from(decision.ordinal).ok()? };
                frames.push(self.enter(Children::Enum(decision.payload_input), ty, at, kind)?);
                return Some(VisitOutcome::Deferred);
            }
        };
        Some(VisitOutcome::Value(value?))
    }
}
