//! Read-only successor views; no existing aggregate-v1 type IDs escape.

use super::{Record, VerifiedLayouts};
use crate::VerifiedKind;
use zryna_source::SourceMapIdentity;

/// One complete-universe and exact-source-bound successor type identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeId {
    pub(super) source_map: SourceMapIdentity,
    pub(super) universe: [u8; 32],
    pub(super) index: u32,
}

impl TypeId {
    /// Canonical unsigned-key-ordered index within the issuing universe.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }
}

/// Immutable closed type record bound to its issuing layouts.
#[derive(Clone, Copy, Debug)]
pub struct TypeView<'a> {
    pub(super) layouts: &'a VerifiedLayouts,
    pub(super) record: &'a Record,
}

impl<'a> TypeView<'a> {
    fn identity(self, index: u32) -> TypeId {
        TypeId { source_map: self.layouts.source_map, universe: self.layouts.universe, index }
    }
    /// Issuing complete type identity.
    #[must_use]
    pub fn id(self) -> TypeId {
        self.identity(self.record.physical.id.index)
    }
    /// Closed physical category, without conversion to an older layout identity.
    #[must_use]
    pub const fn category(self) -> crate::TypeCategory {
        self.record.physical.category()
    }
    /// Exact branded element or payload of a closed container.
    #[must_use]
    pub fn referenced_type(self) -> Option<TypeId> {
        match self.record.physical.kind {
            VerifiedKind::FixedArray { element, .. } | VerifiedKind::Vec { element } => {
                Some(self.identity(element.index))
            }
            VerifiedKind::Shared { payload } | VerifiedKind::Weak { payload } => {
                Some(self.identity(payload.index))
            }
            _ => None,
        }
    }
    /// Complete canonical type key.
    #[must_use]
    pub fn key(self) -> &'a [u8] {
        &self.record.key
    }
    /// Selected target's stored size.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.record.physical.size
    }
    /// Selected target's stored alignment.
    #[must_use]
    pub const fn alignment(self) -> u64 {
        self.record.physical.alignment
    }
    /// Derived v1 drop category for the closed shape.
    #[must_use]
    pub const fn drop_kind(self) -> u32 {
        self.record.physical.drop_kind
    }
    /// Derived v1 runtime category for the closed shape.
    #[must_use]
    pub const fn runtime_kind(self) -> u32 {
        self.record.physical.runtime_kind
    }
    /// Ordered closed argument IDs for generic nominals and compiler families.
    #[must_use]
    pub fn arguments(self) -> impl ExactSizeIterator<Item = TypeId> {
        self.record.arguments.iter().map(move |id| self.identity(*id))
    }
    /// Source-ordered struct fields: ordinal, exact closed type ID, byte offset.
    #[must_use]
    pub fn fields(self) -> impl ExactSizeIterator<Item = (u32, TypeId, u64)> {
        let fields = match &self.record.physical.kind {
            VerifiedKind::Struct { fields, .. } => fields.as_slice(),
            _ => &[],
        };
        fields.iter().map(move |field| (field.ordinal, self.identity(field.ty.index), field.offset))
    }
    /// Source/family-ordered enum variants: ordinal and exact optional payload ID.
    #[must_use]
    pub fn variants(self) -> impl ExactSizeIterator<Item = (u32, Option<TypeId>)> {
        let variants = match &self.record.physical.kind {
            VerifiedKind::Enum { variants, .. } => variants.as_slice(),
            _ => &[],
        };
        variants.iter().map(move |variant| {
            (variant.ordinal, variant.payload.map(|id| self.identity(id.index)))
        })
    }
    /// Enum payload offset and maximum active payload size, absent for other forms.
    #[must_use]
    pub const fn payload(self) -> Option<(u64, u64)> {
        match self.record.physical.kind {
            VerifiedKind::Enum { payload_offset, payload_size, .. } => {
                Some((payload_offset, payload_size))
            }
            _ => None,
        }
    }
}
