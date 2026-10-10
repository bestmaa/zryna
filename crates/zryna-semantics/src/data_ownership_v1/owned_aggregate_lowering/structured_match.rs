use zryna_ir::data_ownership_v1::raw;
use zryna_layout::TypeCategory;
use zryna_source::Span;
use zryna_syntax::v4::{self as syntax, RawDataDeclarationKind, RawExpressionKind, RawMatchArm};

use super::super::diagnostics::span;
use super::structured_graph::StructuredGraph;
use super::{Binding, PrivateOwnedAggregateLowerer, Ty};

pub(super) struct MatchPlan {
    ty: Ty,
    pub(super) arms: Vec<(u32, RawMatchArm, Option<Ty>)>,
}

impl PrivateOwnedAggregateLowerer<'_, '_, '_> {
    pub(super) fn match_plan(&mut self, arms: &[RawMatchArm], at: Span) -> Option<MatchPlan> {
        let Some(first) = arms.first() else {
            self.errors.at(
                "ZRYNA-M3009",
                at,
                "match does not cover any enum variant",
                "provide one arm for every declared variant",
            );
            return None;
        };
        let Some(declaration) = self.declarations.iter().find(|declaration| {
            declaration.module == self.module && declaration.name == first.type_name.text
        }) else {
            self.errors.at(
                "ZRYNA-M3009",
                at,
                "match does not name an exact declared enum",
                "use the scrutinee's exact enum on every arm",
            );
            return None;
        };
        let ty = self.node_types.get(declaration.node.0 as usize).copied().flatten()?;
        let variants = if self.input.command.is_some()
            && declaration.declaration == self.file.data_declarations().len()
            && declaration.name == "EnvLookupV1"
        {
            vec!["Found", "Missing"]
        } else if let Some(syntax::RawDataDeclaration {
            kind: RawDataDeclarationKind::Enum { variants, .. },
            ..
        }) = self.file.data_declarations().get(declaration.declaration)
        {
            variants.iter().map(|variant| variant.name.text.as_str()).collect()
        } else {
            self.errors.at(
                "ZRYNA-M3009",
                at,
                "match scrutinee is not an enum",
                "match one exact nominal enum",
            );
            return None;
        };
        if arms.len() != variants.len() {
            self.errors.at(
                "ZRYNA-M3009",
                at,
                "match does not cover every enum variant exactly once",
                "provide one arm for every declared variant",
            );
            return None;
        }
        let record = self.layouts.type_by_id(ty.layout)?;
        let mut ordered = vec![None; variants.len()];
        for arm in arms {
            let ordinal = variants.iter().position(|variant| *variant == arm.variant.text);
            let valid = ordinal.is_some_and(|ordinal| ordered[ordinal].is_none())
                && arm.type_name.text == declaration.name;
            if !valid {
                self.errors.at(
                    "ZRYNA-M3009",
                    span(self.input.sources(), arm.span),
                    "match arm repeats or names a foreign variant",
                    "provide each exact declared variant once",
                );
                return None;
            }
            let ordinal = ordinal?;
            let payload = record.variants().get(ordinal)?.payload().and_then(|layout| {
                self.node_types.iter().flatten().find(|ty| ty.layout == layout).copied()
            });
            if payload.is_some() != arm.binding.is_some() {
                self.errors.at(
                    "ZRYNA-M3009",
                    span(self.input.sources(), arm.span),
                    "match payload binding does not match its variant",
                    "bind exactly one name for a payload variant and none otherwise",
                );
                return None;
            }
            ordered[ordinal] = Some((u32::try_from(ordinal).ok()?, arm.clone(), payload));
        }
        Some(MatchPlan { ty, arms: ordered.into_iter().collect::<Option<Vec<_>>>()? })
    }

    pub(super) fn structured_value(
        &mut self,
        id: u32,
        result: Ty,
        graph: &mut StructuredGraph,
    ) -> Option<raw::ValueId> {
        let expression = self.expression(id)?.clone();
        let RawExpressionKind::Match { scrutinee, arms, .. } = expression.kind else {
            return self.structured_operand(id, result, graph);
        };
        let at = span(self.input.sources(), expression.span);
        let plan = self.match_plan(&arms, at)?;
        self.join_state(at)?;
        let value = self.structured_value(scrutinee, plan.ty, graph)?;
        let place = self.materialize_match_value(value, plan.ty, at)?;
        let incoming = self.join_state(at)?;
        let bindings = self.bindings.clone();
        let origin = graph.terminate(
            self,
            at,
            raw::Terminator::EnumMatch {
                place,
                arms: plan
                    .arms
                    .iter()
                    .map(|(variant, _, _)| raw::EnumArm {
                        variant: *variant,
                        edge: raw::Edge { target: raw::BlockId(0), arguments: Vec::new() },
                    })
                    .collect(),
            },
        )?;
        let mut targets = Vec::new();
        let mut jumps = Vec::new();
        let mut joined = None;
        for (variant, arm, payload) in plan.arms {
            self.restore_join(&incoming);
            let target = graph.next(self, at)?;
            targets.push(target);
            if let (Some(binding), Some(ty)) = (arm.binding, payload) {
                if self.bindings.keys().any(|name| name.eq_ignore_ascii_case(&binding.text)) {
                    self.errors.at(
                        "ZRYNA-M3002",
                        span(self.input.sources(), binding.span),
                        "match binding collides with a preceding binding",
                        "choose one portable distinct payload binding",
                    );
                    return None;
                }
                if !self.resource_usage().places(1, at, self.errors) {
                    return None;
                }
                let projection = raw::PlaceId(u32::try_from(self.places.len()).ok()?);
                self.places.push(raw::Place {
                    id: projection,
                    ty: ty.ir,
                    span: span(self.input.sources(), binding.span),
                    kind: raw::PlaceKind::EnumPayload { base: place, variant },
                });
                self.bindings
                    .insert(binding.text, Binding { ty, place: projection, mutable: false });
            }
            let result_value = self.structured_value(arm.value, result, graph)?;
            self.finish_match_scrutinee(place, value, plan.ty, at)?;
            if !result.is_copy() {
                let delta = self.owners.transfer(result_value)?;
                self.preparation_facts.apply(delta);
            }
            self.bindings.clone_from(&bindings);
            if let Some(state) = &joined {
                self.reconcile_join(state, at)?;
            }
            joined = Some(self.join_state(at)?);
            jumps.push(graph.terminate(
                self,
                at,
                raw::Terminator::Jump(raw::Edge {
                    target: raw::BlockId(0),
                    arguments: vec![result_value],
                }),
            )?);
        }
        let raw::Terminator::EnumMatch { arms, .. } =
            &mut graph.blocks[origin].terminator.as_mut()?.kind
        else {
            return None;
        };
        for (arm, target) in arms.iter_mut().zip(targets) {
            arm.edge.target = target;
        }
        let join = graph.next(self, at)?;
        for jump in jumps {
            graph.retarget_jump(jump, join);
        }
        graph.result_parameter(self, result, at)
    }

    fn finish_match_scrutinee(
        &mut self,
        place: raw::PlaceId,
        value: raw::ValueId,
        ty: Ty,
        at: Span,
    ) -> Option<()> {
        if ty.is_copy() {
            return self.restore_copy_match(place, value, at);
        }
        if !ty.is_copy() {
            if !self.emit_effect(at, raw::InstructionKind::DropPlace { place }) {
                return None;
            }
            let delta = self.owners.consume_owner(place)?;
            self.preparation_facts.apply(delta);
            self.partial_roots.remove(&place);
            let places = &self.places;
            self.moved_projections.retain(|moved| {
                let mut current = *moved;
                loop {
                    if current == place {
                        return false;
                    }
                    let Some(parent) = places
                        .get(current.0 as usize)
                        .and_then(|place| super::availability::parent_kind(&place.kind))
                    else {
                        return true;
                    };
                    current = parent;
                }
            });
        }
        Some(())
    }

    fn restore_copy_match(
        &mut self,
        place: raw::PlaceId,
        value: raw::ValueId,
        at: Span,
    ) -> Option<()> {
        if !self.preflight_transition(3, at) {
            return None;
        }
        let borrow = raw::BorrowId(self.preparation_facts.next_borrow);
        let Some(next) = self.preparation_facts.next_borrow.checked_add(1) else {
            self.errors.at(
                "ZRYNA-M3201",
                at,
                "match restoration exceeds the checked borrow identity limit",
                "reduce the number of borrow operations",
            );
            return None;
        };
        self.preparation_facts.next_borrow = next;
        self.emit_prepared_effect(
            at,
            raw::InstructionKind::BeginBorrow(raw::BorrowDefinition {
                id: borrow,
                place,
                access: raw::BorrowAccess::Exclusive,
                span: at,
            }),
        );
        self.emit_prepared_effect(at, raw::InstructionKind::BorrowWrite { borrow, value });
        self.emit_prepared_effect(at, raw::InstructionKind::EndBorrow { borrow });
        Some(())
    }

    fn materialize_match_value(
        &mut self,
        value: raw::ValueId,
        ty: Ty,
        at: Span,
    ) -> Option<raw::PlaceId> {
        if ty.category != TypeCategory::Enum {
            return None;
        }
        if !ty.is_copy() {
            return self.owners.owner(value);
        }
        if !self.resource_usage().places(1, at, self.errors) || !self.preflight_transition(1, at) {
            return None;
        }
        let place = raw::PlaceId(u32::try_from(self.places.len()).ok()?);
        self.places.push(raw::Place {
            id: place,
            ty: ty.ir,
            span: at,
            kind: raw::PlaceKind::Temporary(value),
        });
        self.emit_prepared_effect(at, raw::InstructionKind::InitializePlace { place, value });
        self.preparation_facts.initialized_copy_roots.insert(place);
        Some(place)
    }
}
