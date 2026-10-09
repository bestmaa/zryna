use super::{DeclarationError, require};
use std::collections::BTreeMap;
use zryna_syntax::native_c_v0::raw::{
    AbiType, Access, BorrowEnd, Category, DeclarationSet, Direction, Effects, Encoding, Mode,
    NullRule, Owner,
};

pub(super) fn check(document: &DeclarationSet) -> Result<(), DeclarationError> {
    let operations: BTreeMap<_, _> =
        document.operations.iter().map(|operation| (operation.key.as_str(), operation)).collect();
    for library in &document.libraries {
        let prefix = format!("{}/", library.id);
        require(
            library.kinds.iter().all(|kind| kind.starts_with(&prefix)),
            "ZRYNA-C4105",
            "nominal-library-kind",
        )?;
        for allocator in &library.allocators {
            require(
                allocator.id.starts_with(&prefix) && library.kinds.contains(&allocator.kind),
                "ZRYNA-C4105",
                "allocator-kind-identity",
            )?;
            let create = operations.get(allocator.create.as_str());
            let release = operations.get(allocator.release.as_str());
            require(
                create.is_some_and(|operation| {
                    operation.direction == Direction::Import
                        && operation.library == library.id
                        && operation.resources.iter().any(|resource| {
                            resource.access == Access::Create
                                && resource.allocator == allocator.id
                                && resource.kind == allocator.kind
                        })
                }) && release.is_some_and(|operation| {
                    operation.direction == Direction::Import
                        && operation.library == library.id
                        && operation.mode == Mode::Void
                        && operation.result == AbiType::Unit
                        && operation.parameters.len() == 1
                        && operation.resources.len() == 1
                        && operation.resources[0].access == Access::Consume
                        && operation.resources[0].allocator == allocator.id
                        && operation.resources[0].kind == allocator.kind
                }),
                "ZRYNA-C4105",
                "allocator-reference",
            )?;
        }
    }
    for operation in &document.operations {
        let library = document.libraries.iter().find(|library| library.id == operation.library);
        if operation.effects == Effects::Total {
            require(
                operation.mode == Mode::Direct
                    && operation.resources.is_empty()
                    && operation
                        .parameters
                        .iter()
                        .all(|parameter| super::policy::scalar(parameter.abi).is_some())
                    && super::policy::scalar(operation.result).is_some(),
                "ZRYNA-C4104",
                "total-scalar-signature",
            )?;
        }
        for (index, resource) in operation.resources.iter().enumerate() {
            check_resource(operation, library, index, resource)?;
        }
        for (index, parameter) in operation.parameters.iter().enumerate() {
            let index = u8::try_from(index)
                .map_err(|_| DeclarationError { code: "ZRYNA-C4107", detail: "parameter-index" })?;
            if let Some(resource) = parameter.resource {
                require(
                    operation
                        .resources
                        .get(usize::from(resource))
                        .is_some_and(|resource| resource.slots.contains(&index)),
                    "ZRYNA-C4105",
                    "parameter-resource",
                )?;
            } else {
                require(
                    matches!(
                        parameter.abi,
                        AbiType::CI32 | AbiType::CInt | AbiType::Bool32 | AbiType::I32Out
                    ),
                    "ZRYNA-C4105",
                    "pointer-or-count-policy",
                )?;
            }
            if matches!(
                parameter.abi,
                AbiType::CI32 | AbiType::CInt | AbiType::Bool32 | AbiType::I32Out
            ) {
                require(parameter.resource.is_none(), "ZRYNA-C4105", "scalar-resource")?;
            }
        }
    }
    Ok(())
}

fn check_resource(
    operation: &zryna_syntax::native_c_v0::raw::Operation,
    library: Option<&zryna_syntax::native_c_v0::raw::Library>,
    index: usize,
    resource: &zryna_syntax::native_c_v0::raw::Resource,
) -> Result<(), DeclarationError> {
    let index = u8::try_from(index)
        .map_err(|_| DeclarationError { code: "ZRYNA-C4107", detail: "resource-index" })?;
    require(
        !resource.slots.is_empty()
            && resource.slots.iter().all(|slot| {
                operation
                    .parameters
                    .get(usize::from(*slot))
                    .is_some_and(|parameter| parameter.resource == Some(index))
            }),
        "ZRYNA-C4105",
        "resource-slot",
    )?;
    owner_policy(operation, resource)?;
    let types: Vec<_> = resource
        .slots
        .iter()
        .filter_map(|slot| {
            operation.parameters.get(usize::from(*slot)).map(|parameter| parameter.abi)
        })
        .collect();
    if resource.allocator == "none" {
        require(
            resource.kind == "borrowed-bytes"
                && resource.release == "none"
                && resource.access == Access::Read
                && resource.null_rule == NullRule::NullZero
                && resource.max_bytes.is_some()
                && resource.encoding != Encoding::None
                && types == [AbiType::BytesIn, AbiType::Count],
            "ZRYNA-C4105",
            "borrow-policy",
        )?;
        return Ok(());
    }
    let allocator = library
        .and_then(|library| {
            library.allocators.iter().find(|allocator| allocator.id == resource.allocator)
        })
        .ok_or(DeclarationError { code: "ZRYNA-C4105", detail: "allocator-pair" })?;
    require(
        allocator.kind == resource.kind
            && allocator.release == resource.release
            && (resource.access != Access::Create || allocator.create == operation.key),
        "ZRYNA-C4105",
        "allocator-pair",
    )?;
    if allocator.category == Category::Handle {
        let expected =
            if resource.access == Access::Create { AbiType::HandleOut } else { AbiType::HandleIn };
        require(
            types == [expected]
                && resource.null_rule == NullRule::Nonnull
                && resource.max_bytes.is_none()
                && resource.encoding == Encoding::None
                && resource.expected_length_slot.is_none(),
            "ZRYNA-C4105",
            "handle-policy",
        )?;
    } else {
        require(
            resource.max_bytes.is_some() && resource.encoding != Encoding::None,
            "ZRYNA-C4105",
            "byte-policy",
        )?;
        if resource.access == Access::Create {
            require(
                types == [AbiType::BytesOwnedOut, AbiType::CountOut]
                    && resource.null_rule == NullRule::NullZero,
                "ZRYNA-C4105",
                "byte-output-signature",
            )?;
            if let Some(count) = resource.expected_length_slot {
                require(
                    operation
                        .parameters
                        .get(usize::from(count))
                        .is_some_and(|parameter| parameter.abi == AbiType::Count)
                        && operation.resources.iter().any(|input| {
                            input.kind == "borrowed-bytes"
                                && input.access == Access::Read
                                && input.slots.get(1) == Some(&count)
                                && input.max_bytes == resource.max_bytes
                                && input.encoding == resource.encoding
                        }),
                    "ZRYNA-C4105",
                    "expected-length-slot",
                )?;
            }
        } else {
            require(
                resource.access == Access::Consume
                    && types == [AbiType::BytesRelease]
                    && resource.null_rule == NullRule::Nonnull,
                "ZRYNA-C4105",
                "byte-release-signature",
            )?;
        }
    }
    Ok(())
}

fn owner_policy(
    operation: &zryna_syntax::native_c_v0::raw::Operation,
    resource: &zryna_syntax::native_c_v0::raw::Resource,
) -> Result<(), DeclarationError> {
    require(resource.valid_pointer_guarantee, "ZRYNA-C4105", "pointer-guarantee")?;
    match resource.access {
        Access::Read => require(
            resource.owner_before == Owner::Caller
                && resource.owner_after == Owner::Caller
                && resource.borrow_end == BorrowEnd::Return
                && !resource.fresh
                && !resource.releasable_on_malformed
                && resource.expected_length_slot.is_none(),
            "ZRYNA-C4105",
            "read-owner-policy",
        )?,
        Access::Create => require(
            resource.owner_before == Owner::None
                && resource.owner_after == Owner::Caller
                && resource.borrow_end == BorrowEnd::None
                && resource.fresh
                && operation.mode == Mode::Status,
            "ZRYNA-C4105",
            "create-owner-policy",
        )?,
        Access::Consume => require(
            resource.owner_before == Owner::Caller
                && resource.owner_after == Owner::Consumed
                && resource.borrow_end == BorrowEnd::None
                && !resource.fresh
                && !resource.releasable_on_malformed
                && resource.expected_length_slot.is_none()
                && resource.release == operation.key
                && operation.mode == Mode::Void
                && operation.parameters.len() == 1
                && operation.resources.len() == 1,
            "ZRYNA-C4105",
            "consume-owner-policy",
        )?,
    }
    Ok(())
}
