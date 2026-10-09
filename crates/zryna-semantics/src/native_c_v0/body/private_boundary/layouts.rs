//! Existing dual layout and runtime declaration issuers, bound to the original source map.

use super::super::ValueType;
use super::{BoundaryError, PrivateFault};
use zryna_layout::{StorageTarget, TypeCategory, TypeId, VerifiedLayouts, raw};
use zryna_ownership_runtime_abi::raw as runtime_raw;
use zryna_ownership_runtime_abi::{
    LogicalOperation, OperationIdentity, RuntimeStatus, VerifiedOwnershipRuntimeAbi,
    VerifiedStatusDisposition, raw_v1, verify_v1,
};
use zryna_source::SourceMap;

#[derive(Clone, Debug)]
pub(in crate::native_c_v0::body) struct LayoutAuthority {
    pub(in crate::native_c_v0::body) linear: VerifiedLayouts,
    pub(in crate::native_c_v0::body) native: VerifiedLayouts,
    pub(in crate::native_c_v0::body) runtime: VerifiedOwnershipRuntimeAbi,
}
impl LayoutAuthority {
    pub(super) fn derive(sources: &SourceMap) -> Result<Self, BoundaryError> {
        let modules = (0..sources.len())
            .map(|index| {
                let ordinal = u32::try_from(index)
                    .map_err(|_| BoundaryError::budget("boundary-module-budget"))?;
                let source_file = sources
                    .verify_file_id(ordinal)
                    .map_err(|_| BoundaryError::source("boundary-source-file"))?;
                Ok(raw::Module { id: raw::ModuleId(ordinal), source_file, data_declarations: 0 })
            })
            .collect::<Result<Vec<_>, BoundaryError>>()?;
        let kinds = [
            raw::TypeKind::Bool,
            raw::TypeKind::I32,
            raw::TypeKind::String,
            raw::TypeKind::Vec { element: raw::NodeId(1) },
        ];
        let graph = raw::Graph {
            modules,
            types: kinds
                .into_iter()
                .zip(0..4)
                .map(|(kind, id)| raw::TypeNode { id: raw::NodeId(id), span: None, kind })
                .collect(),
            program_roots: (0..4).map(raw::NodeId).collect(),
        };
        let linear = zryna_layout::verify(&graph, sources, StorageTarget::Linear32V1)
            .map_err(BoundaryError::layout)?;
        let native = zryna_layout::verify(&graph, sources, StorageTarget::LinuxX8664V1)
            .map_err(BoundaryError::layout)?;
        let claim = raw_v1(&linear, &native);
        let runtime = verify_v1(claim, &linear, &native).map_err(BoundaryError::runtime)?;
        let result = Self { linear, native, runtime };
        result.check(sources)?;
        Ok(result)
    }

    pub(super) fn check(&self, sources: &SourceMap) -> Result<(), BoundaryError> {
        if self.linear.source_map_identity() != sources.identity()
            || self.native.source_map_identity() != sources.identity()
            || self.linear.target() != StorageTarget::Linear32V1
            || self.native.target() != StorageTarget::LinuxX8664V1
            || self.linear.universe_identity() != self.native.universe_identity()
            || self.runtime.type_universe_identity() != self.native.universe_identity()
            || self.runtime.linear32_fingerprint() != *self.linear.fingerprint()
            || self.runtime.linux_x86_64_fingerprint() != *self.native.fingerprint()
            || self.native.types().len() != 4
            || self.linear.types().len() != 4
        {
            return Err(BoundaryError::source("boundary-layout-issuer"));
        }
        for ty in [ValueType::Bool, ValueType::I32, ValueType::String, ValueType::VecI32] {
            let native_id = self.type_id(ty)?;
            let linear = self
                .linear
                .type_by_id(native_id)
                .ok_or_else(|| BoundaryError::source("boundary-layout-universe"))?;
            if linear.category()
                != self
                    .native
                    .type_by_id(native_id)
                    .ok_or_else(|| BoundaryError::source("boundary-layout-category"))?
                    .category()
            {
                return Err(BoundaryError::source("boundary-layout-category"));
            }
        }
        for target in [StorageTarget::Linear32V1, StorageTarget::LinuxX8664V1] {
            let (stride, alignment) = self.element(target)?;
            if stride != 4 || alignment != 4 {
                return Err(BoundaryError::source("boundary-i32-storage"));
            }
        }
        Ok(())
    }

    pub(super) fn type_id(&self, ty: ValueType) -> Result<TypeId, BoundaryError> {
        let category = match ty {
            ValueType::Bool => TypeCategory::Bool,
            ValueType::I32 => TypeCategory::I32,
            ValueType::String => TypeCategory::String,
            ValueType::VecI32 => TypeCategory::Vec,
            _ => return Err(BoundaryError::typed("boundary-storage-category")),
        };
        let mut matches = self.native.types().filter(|record| record.category() == category);
        let record =
            matches.next().ok_or_else(|| BoundaryError::source("boundary-storage-type"))?;
        if matches.next().is_some() {
            return Err(BoundaryError::source("boundary-storage-ambiguity"));
        }
        if category == TypeCategory::Vec
            && record.referenced_type() != Some(self.type_id(ValueType::I32)?)
        {
            return Err(BoundaryError::typed("boundary-vector-element"));
        }
        Ok(record.id())
    }

    pub(super) fn element(&self, target: StorageTarget) -> Result<(u64, u64), BoundaryError> {
        let element = self.type_id(ValueType::I32)?;
        self.runtime
            .element_layouts()
            .find(|record| record.target() == target && record.element() == element)
            .map(|record| (record.stride(), record.alignment()))
            .ok_or_else(|| BoundaryError::source("boundary-element-issuer"))
    }

    pub(super) fn native_bits(&self) -> Result<u8, BoundaryError> {
        let record = self
            .runtime
            .records()
            .find(|record| {
                record.target() == runtime_raw::RecordTarget::LinuxX8664V1
                    && record.kind() == &runtime_raw::RecordKind::VecHandle
            })
            .ok_or_else(|| BoundaryError::source("boundary-native-record"))?;
        let pointer = record
            .fields()
            .iter()
            .find(|field| field.role == runtime_raw::FieldRole::Pointer)
            .ok_or_else(|| BoundaryError::source("boundary-native-pointer"))?;
        let length = record
            .fields()
            .iter()
            .find(|field| field.role == runtime_raw::FieldRole::Length)
            .ok_or_else(|| BoundaryError::source("boundary-native-length"))?;
        if pointer.size != length.size || pointer.size != 8 {
            return Err(BoundaryError::source("boundary-native-lane"));
        }
        pointer
            .size
            .checked_mul(8)
            .and_then(|bits| u8::try_from(bits).ok())
            .ok_or_else(|| BoundaryError::source("boundary-native-lane"))
    }

    pub(super) fn operation(
        &self,
        wanted: LogicalOperation,
    ) -> Result<OperationIdentity, BoundaryError> {
        self.runtime
            .operations()
            .find(|record| record.operation() == wanted)
            .map(zryna_ownership_runtime_abi::VerifiedOperation::id)
            .ok_or_else(|| BoundaryError::source("boundary-operation-issuer"))
    }

    pub(super) fn faults(
        &self,
        operation: LogicalOperation,
    ) -> Result<Vec<PrivateFault>, BoundaryError> {
        let operation = self.operation(operation)?;
        [RuntimeStatus::Allocation, RuntimeStatus::Capacity]
            .into_iter()
            .map(|status| {
                let declaration = self
                    .runtime
                    .status_declarations()
                    .find(|declaration| declaration.status() == status)
                    .ok_or_else(|| BoundaryError::source("boundary-status-issuer"))?;
                if declaration.disposition() != VerifiedStatusDisposition::ControlledTrap
                    || declaration.trap_identity().is_none()
                {
                    return Err(BoundaryError::owned("boundary-private-trap-domain"));
                }
                Ok(PrivateFault { runtime: self.runtime.identity(), operation, declaration })
            })
            .collect()
    }
}
