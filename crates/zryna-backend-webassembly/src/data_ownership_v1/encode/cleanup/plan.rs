use std::collections::{BTreeMap, BTreeSet};
use zryna_ir::data_ownership_v1::{
    PlaceIdentity, VerifiedDropAction, VerifiedFunction, VerifiedPlaceKind,
};
use zryna_layout::{TypeCategory, TypeId, VerifiedLayouts};

use super::super::index_error;

pub(super) struct Plan {
    pub(super) ty: TypeId,
    pub(super) kind: Kind,
}

pub(super) enum Kind {
    Complete,
    Struct(Vec<(u64, Plan)>),
    Enum { active: Option<u32>, payloads: Vec<(u32, u64, Plan)> },
    Array(Vec<Segment>),
}

pub(super) enum Segment {
    Complete { start: u32, end: u32 },
    Projected { index: u32, plan: Plan },
}

struct Selection<'a> {
    places: BTreeMap<u32, VerifiedPlaceKind>,
    initialized: BTreeSet<u32>,
    moved: BTreeSet<u32>,
    ancestors: BTreeSet<u32>,
    variants: BTreeMap<u32, u32>,
    layouts: &'a VerifiedLayouts,
}

impl Plan {
    pub(super) fn derive(
        function: VerifiedFunction<'_>,
        action: &VerifiedDropAction,
        layouts: &VerifiedLayouts,
    ) -> Result<Self, zryna_diagnostics::Diagnostic> {
        let root = action.root().index();
        let ty = function
            .places()
            .find(|place| place.id().index() == root)
            .ok_or_else(index_error)?
            .ty();
        let mut selection = Selection {
            places: function.places().map(|place| (place.id().index(), place.kind())).collect(),
            initialized: action.initialized_projections().map(PlaceIdentity::index).collect(),
            moved: action.moved_projections().map(PlaceIdentity::index).collect(),
            ancestors: BTreeSet::new(),
            variants: action
                .active_variants()
                .map(|item| (item.place().index(), item.variant()))
                .collect(),
            layouts,
        };
        for child in selection.initialized.iter().chain(&selection.moved) {
            let mut current = *child;
            while let Some(base) = selection.places.get(&current).and_then(parent) {
                selection.ancestors.insert(base);
                if base == root {
                    break;
                }
                current = base;
            }
        }
        selection.build(Some(root), ty, true)?.ok_or_else(index_error)
    }
}

impl Selection<'_> {
    fn build(
        &self,
        place: Option<u32>,
        ty: TypeId,
        root: bool,
    ) -> Result<Option<Plan>, zryna_diagnostics::Diagnostic> {
        let layout = self.layouts.type_by_id(ty).ok_or_else(index_error)?;
        if layout.drop_kind() == 0 {
            return Ok(root.then_some(Plan { ty, kind: Kind::Complete }));
        }
        let Some(place) = place else {
            // Partial initialization requires every immediate canonical projection.
            // An omitted child of a live node therefore belongs to its complete owner.
            return Ok(Some(Plan { ty, kind: Kind::Complete }));
        };
        if self.moved.contains(&place)
            || (!root && !self.initialized.contains(&place) && !self.ancestors.contains(&place))
        {
            return Ok(None);
        }
        let descendants = self
            .places
            .keys()
            .copied()
            .filter(|child| self.below(*child, place) && self.active_path(*child, place));
        if descendants.clone().all(|child| self.initialized.contains(&child)) {
            return Ok(Some(Plan { ty, kind: Kind::Complete }));
        }
        let kind = match layout.category() {
            TypeCategory::Struct => {
                let mut children = Vec::new();
                for field in layout.fields() {
                    let child = self.child(place, |kind| matches!(kind,
                        VerifiedPlaceKind::StructField { ordinal, .. } if ordinal == field.ordinal()));
                    if let Some(plan) = self.build(child, field.ty(), false)? {
                        children.push((field.offset(), plan));
                    }
                }
                Kind::Struct(children)
            }
            TypeCategory::Enum => {
                let active = self.variants.get(&place).copied();
                let offset = layout.enum_payload_layout().ok_or_else(index_error)?.0;
                let mut payloads = Vec::new();
                for variant in layout.variants() {
                    if active.is_some_and(|selected| selected != variant.ordinal()) {
                        continue;
                    }
                    let Some(payload) = variant.payload() else {
                        continue;
                    };
                    let child = self.child(place, |kind| matches!(kind,
                        VerifiedPlaceKind::EnumPayload { variant: ordinal, .. } if ordinal == variant.ordinal()));
                    if let Some(plan) = self.build(child, payload, false)? {
                        payloads.push((variant.ordinal(), offset, plan));
                    }
                }
                Kind::Enum { active, payloads }
            }
            TypeCategory::FixedArray => Kind::Array(self.array(place, layout)?),
            _ => return Ok(Some(Plan { ty, kind: Kind::Complete })),
        };
        Ok(Some(Plan { ty, kind }))
    }

    fn array(
        &self,
        place: u32,
        layout: zryna_layout::VerifiedType<'_>,
    ) -> Result<Vec<Segment>, zryna_diagnostics::Diagnostic> {
        let ty = layout.referenced_type().ok_or_else(index_error)?;
        let length = layout
            .array_length()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(index_error)?;
        let mut children = self
            .places
            .iter()
            .filter_map(|(id, kind)| match kind {
                VerifiedPlaceKind::FixedArrayConstant { base, index } if base.index() == place => {
                    Some((*index, *id))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        children.sort_unstable();
        let mut segments = Vec::new();
        let mut next = 0;
        for (index, child) in children {
            if next < index {
                segments.push(Segment::Complete { start: next, end: index });
            }
            if let Some(plan) = self.build(Some(child), ty, false)? {
                match plan.kind {
                    Kind::Complete => {
                        segments.push(Segment::Complete { start: index, end: index + 1 });
                    }
                    _ => segments.push(Segment::Projected { index, plan }),
                }
            }
            next = index + 1;
        }
        if next < length {
            segments.push(Segment::Complete { start: next, end: length });
        }
        Ok(segments)
    }

    fn child(&self, base: u32, matches: impl Fn(VerifiedPlaceKind) -> bool) -> Option<u32> {
        self.places
            .iter()
            .find_map(|(id, kind)| (parent(kind) == Some(base) && matches(*kind)).then_some(*id))
    }

    fn below(&self, mut child: u32, root: u32) -> bool {
        while let Some(base) = self.places.get(&child).and_then(parent) {
            if base == root {
                return true;
            }
            child = base;
        }
        false
    }

    fn active_path(&self, mut child: u32, root: u32) -> bool {
        while let Some(kind) = self.places.get(&child) {
            if let VerifiedPlaceKind::EnumPayload { base, variant } = kind
                && self.variants.get(&base.index()).is_some_and(|active| active != variant)
            {
                return false;
            }
            let Some(base) = parent(kind) else {
                break;
            };
            if base == root {
                break;
            }
            child = base;
        }
        true
    }
}

fn parent(kind: &VerifiedPlaceKind) -> Option<u32> {
    match kind {
        VerifiedPlaceKind::StructField { base, .. }
        | VerifiedPlaceKind::EnumPayload { base, .. }
        | VerifiedPlaceKind::FixedArrayConstant { base, .. } => Some(base.index()),
        _ => None,
    }
}
