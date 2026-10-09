use super::{
    CleanupRecipe, CleanupUsage, Leaf, MaterializedProjectionTopology, OwnedCleanupPlanContext,
    PreparationPlan, PrivateOwnedAggregateLowerer, ProjectionDescriptor, Span, Ty, project, raw,
};

pub(super) fn assert_final(
    lowerer: &PrivateOwnedAggregateLowerer<'_, '_, '_>,
    plan: &PreparationPlan<'_>,
    original_places: usize,
    last_result: Option<(raw::ValueId, Ty)>,
    visits: usize,
) {
    assert_eq!(
        last_result,
        Some((plan.result, plan.result_type)),
        "prepared root result and exact type"
    );
    assert_eq!(visits, plan.visits, "one consumed result per classified expression");
    assert_eq!(lowerer.owners, plan.owners, "prepared owner map and pending order");
    assert_eq!(lowerer.projections, plan.projections, "prepared canonical projection identities");
    assert_eq!(lowerer.moved_projections, plan.moved, "prepared moved projection mask");
    assert_eq!(lowerer.partial_roots, plan.partial, "prepared partial root mask");
    assert_eq!(lowerer.preparation_facts, plan.facts, "prepared cleanup and String byte facts");
    assert_eq!(
        lowerer.places.len() - original_places,
        plan.places.len(),
        "prepared place suffix length"
    );
    for (index, expected) in plan.places.iter().enumerate() {
        let actual = &lowerer.places[original_places + index];
        assert_eq!(actual.id.0 as usize, original_places + index, "dense prepared place");
        assert_eq!(actual.ty, expected.ty.ir, "prepared place type");
        assert_eq!(actual.span, expected.at, "prepared first place span");
        assert_eq!(actual.kind, expected.kind, "prepared place topology");
    }
}

impl PrivateOwnedAggregateLowerer<'_, '_, '_> {
    pub(super) fn consume_prepared_prefix(
        &mut self,
        id: raw::PlaceId,
        descriptor: ProjectionDescriptor,
    ) {
        let reserved_places = self.reserved_constructor_places();
        let actual = project(
            &mut MaterializedProjectionTopology {
                projections: &mut self.projections,
                places: &mut self.places,
                reserved_places,
            },
            descriptor,
            self.errors,
        )
        .expect("prepared projection capacity");
        assert_eq!(actual, id, "prepared projection identity");
    }

    #[allow(clippy::too_many_lines)]
    pub(in crate::data_ownership_v1::owned_aggregate_lowering) fn consume_prepared_leaf(
        &mut self,
        leaf: Leaf<'_>,
        ty: Ty,
        at: Span,
    ) -> Option<super::super::super::state::Emission> {
        let bytes = match &leaf {
            Leaf::String { bytes, .. } => {
                Some(u64::try_from(bytes.len()).expect("String byte length"))
            }
            Leaf::StringClone { source, bytes, .. } => {
                assert_eq!(
                    super::super::super::super::owned_string_read::StringBytes::from_known(
                        self.preparation_facts.string_bytes.get(&source.place).copied()
                    ),
                    *bytes,
                    "String clone exact byte witness"
                );
                bytes.known()
            }
            Leaf::StringConcat { bytes, .. } => bytes.known(),
            _ => None,
        };
        let shared_value = match &leaf {
            Leaf::SharedConstruct { value, .. } => Some(*value),
            _ => None,
        };
        let mut emission = match leaf {
            Leaf::BorrowRead(borrow) => {
                self.emit_recorded(ty, at, raw::InstructionKind::BorrowRead { borrow })
            }
            Leaf::IndexedCopy { source, index, cleanup } => self.emit_recorded(
                ty,
                at,
                raw::InstructionKind::VecIndexCopy { place: source, index, cleanup },
            ),
            Leaf::IndexedClone { borrow, cleanup, prefix } => self.emit_recorded(
                ty,
                at,
                raw::InstructionKind::GenericCloneBorrow {
                    borrow,
                    cleanup,
                    prefix_cleanup: prefix,
                },
            ),
            Leaf::HandleAwareIndexedClone { borrow, cleanup, prefix } => self.emit_recorded(
                ty,
                at,
                raw::InstructionKind::HandleAwareCloneBorrow {
                    borrow,
                    cleanup,
                    prefix_cleanup: prefix,
                },
            ),
            Leaf::Bool(value) => {
                self.emit_recorded(ty, at, raw::InstructionKind::BoolLiteral(value))
            }
            Leaf::I32(value) => self.emit_recorded(ty, at, raw::InstructionKind::I32Literal(value)),
            Leaf::String { bytes, cleanup } => self.emit_recorded(
                ty,
                at,
                raw::InstructionKind::StringFromUtf8 { bytes: bytes.to_vec(), cleanup },
            ),
            Leaf::Environment { key, cleanup } => {
                self.emit_recorded(ty, at, raw::InstructionKind::EnvironmentLookup { key, cleanup })
            }
            Leaf::Reference(decision) => self.emit_reference_recorded(decision, ty, at),
            Leaf::Projection { source, operation } => {
                self.emit_projection_recorded(source, ty, at, &operation)
            }
            Leaf::StringClone { source, cleanup, .. } => {
                self.emit_prepared_string_clone(source, ty, at, cleanup)
            }
            Leaf::StringConcat { left, right, cleanup, .. } => self.emit_recorded(
                ty,
                at,
                raw::InstructionKind::StringConcat { left, right, cleanup },
            ),
            Leaf::AggregateClone { source, cleanup, prefix } => {
                self.emit_prepared_aggregate_clone(source, ty, at, cleanup, prefix)
            }
            Leaf::GenericClone { source, cleanup, prefix } => self.emit_recorded(
                ty,
                at,
                raw::InstructionKind::GenericClonePlace {
                    place: source,
                    cleanup,
                    prefix_cleanup: prefix,
                },
            ),
            Leaf::HandleAwareClone { source, cleanup, prefix } => self.emit_recorded(
                ty,
                at,
                raw::InstructionKind::HandleAwareClonePlace {
                    place: source,
                    cleanup,
                    prefix_cleanup: prefix,
                },
            ),
            Leaf::SharedConstruct { value, cleanup } => {
                self.emit_recorded(ty, at, raw::InstructionKind::SharedConstruct { value, cleanup })
            }
            Leaf::SharedClone { source, cleanup } => self.emit_recorded(
                ty,
                at,
                raw::InstructionKind::SharedClone { place: source, cleanup },
            ),
            Leaf::WeakDowngrade { source, cleanup } => self.emit_recorded(
                ty,
                at,
                raw::InstructionKind::WeakDowngrade { place: source, cleanup },
            ),
            Leaf::WeakClone { source, cleanup } => self.emit_recorded(
                ty,
                at,
                raw::InstructionKind::WeakClone { place: source, cleanup },
            ),
        }?;
        if let Some(value) = shared_value
            && self.owners.owner(value).is_some()
        {
            emission.owners.push(self.owners.transfer(value)?);
        }
        for delta in &emission.owners {
            super::super::super::super::owner_state::apply_owner_delta(
                &mut self.preparation_facts.string_bytes,
                *delta,
            );
        }
        if let (Some(owner), Some(bytes)) = (self.owners.owner(emission.value), bytes) {
            self.preparation_facts.string_bytes.insert(owner, bytes);
        }
        Some(emission)
    }

    pub(super) fn consume_prepared_cleanup(
        &mut self,
        id: raw::CleanupPlanId,
        actions: usize,
        prefix: Option<raw::PlaceId>,
        at: Span,
    ) {
        let usage = CleanupUsage {
            plans: self.cleanup_plans.len(),
            actions: self.cleanup_actions,
            reserved_plans: self.preparation_facts.held_cleanup[0],
            reserved_actions: self.preparation_facts.held_cleanup[1],
        };
        let actual = match prefix {
            None => self.push_cleanup(at, None),
            Some(owner) => self.push_aggregate_clone_prefix_cleanup(at, owner),
        }
        .expect("prepared cleanup emission");
        assert_eq!(actual, id, "prepared cleanup identity");
        // The owner state has not changed across this event. Expand the same recipe only
        // after real emission, so comparison is linear in the actually charged actions.
        let recipe = match prefix {
            None => CleanupRecipe::reverse(
                &usage,
                self.owners.pending(),
                None,
                OwnedCleanupPlanContext::Aggregate,
                at,
                self.errors,
            ),
            Some(owner) => {
                CleanupRecipe::aggregate_prefix(usage.plans, self.owners.pending(), owner)
            }
        }
        .expect("prepared cleanup recipe");
        assert_eq!(recipe.action_count, actions, "prepared cleanup action count");
        let actual = &self.cleanup_plans[id.0 as usize];
        assert_eq!(actual.id, id);
        assert!(
            actual.actions.iter().copied().eq(recipe.into_actions()),
            "prepared cleanup action order"
        );
    }
}

pub(super) fn check_cleanup_link(
    leaf: &Leaf<'_>,
    events: &[(raw::CleanupPlanId, Option<raw::PlaceId>)],
    places: usize,
) {
    match leaf {
        Leaf::String { cleanup, .. }
        | Leaf::Environment { cleanup, .. }
        | Leaf::IndexedCopy { cleanup, .. }
        | Leaf::StringClone { cleanup, .. }
        | Leaf::StringConcat { cleanup, .. } => {
            assert_eq!(events, &[(*cleanup, None)], "fallible leaf cleanup linkage");
        }
        Leaf::AggregateClone { cleanup, prefix, .. }
        | Leaf::IndexedClone { cleanup, prefix, .. }
        | Leaf::HandleAwareIndexedClone { cleanup, prefix, .. }
        | Leaf::GenericClone { cleanup, prefix, .. }
        | Leaf::HandleAwareClone { cleanup, prefix, .. } => {
            let owner = raw::PlaceId(u32::try_from(places).expect("prepared clone owner identity"));
            assert_eq!(
                events,
                &[(*cleanup, None), (*prefix, Some(owner))],
                "clone cleanup role linkage"
            );
        }
        Leaf::SharedConstruct { cleanup, .. }
        | Leaf::SharedClone { cleanup, .. }
        | Leaf::WeakDowngrade { cleanup, .. }
        | Leaf::WeakClone { cleanup, .. } => {
            assert_eq!(events, &[(*cleanup, None)], "handle failure cleanup linkage");
        }
        _ => assert!(events.is_empty(), "infallible leaf has no cleanup events"),
    }
}
