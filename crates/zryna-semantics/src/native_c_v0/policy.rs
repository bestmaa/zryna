use super::{DeclarationError, require};
use std::collections::BTreeSet;
use zryna_syntax::native_c_v0::raw::{
    AbiType, Condition, DeclarationSet, Direction, Effects, Mode, StatusKind,
};

pub(super) fn scalar(carrier: AbiType) -> Option<zryna_abi::raw::Type> {
    match carrier {
        AbiType::CI32 | AbiType::CInt => Some(zryna_abi::raw::Type::I32),
        AbiType::Bool32 => Some(zryna_abi::raw::Type::Bool),
        _ => None,
    }
}

pub(super) fn check(document: &DeclarationSet) -> Result<(), DeclarationError> {
    let mut exports = Vec::new();
    for operation in &document.operations {
        let names: BTreeSet<_> =
            operation.parameters.iter().map(|parameter| &parameter.name).collect();
        require(names.len() == operation.parameters.len(), "ZRYNA-C4104", "duplicate-parameter")?;
        let outputs: Vec<_> = operation
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(index, parameter)| {
                matches!(
                    parameter.abi,
                    AbiType::I32Out
                        | AbiType::HandleOut
                        | AbiType::BytesOwnedOut
                        | AbiType::CountOut
                )
                .then_some(index)
            })
            .map(|index| {
                u8::try_from(index)
                    .map_err(|_| DeclarationError { code: "ZRYNA-C4107", detail: "slot-index" })
            })
            .collect::<Result<Vec<_>, _>>()?;
        check_status(operation, &outputs)?;
        if operation.direction == Direction::Export {
            require(operation.logical_name.len() <= 115, "ZRYNA-C4107", "export-name-bytes")?;
            require(
                operation.mode == Mode::Direct
                    && operation.effects == Effects::Total
                    && operation.resources.is_empty()
                    && operation.statuses.is_empty()
                    && operation.parameters.iter().all(|parameter| {
                        scalar(parameter.abi).is_some() && parameter.resource.is_none()
                    })
                    && scalar(operation.result).is_some(),
                "ZRYNA-C4104",
                "export-surface",
            )?;
            // Reuse only scalar-name/signature defensive checks. This temporary scalar projection
            // is discarded and never becomes a foreign ABI, source-body or executable seal.
            exports.push(zryna_abi::raw::Export::new(
                operation.logical_name.clone(),
                zryna_abi::raw::Signature::new(
                    operation
                        .parameters
                        .iter()
                        .filter_map(|parameter| scalar(parameter.abi))
                        .collect(),
                    scalar(operation.result)
                        .ok_or(DeclarationError { code: "ZRYNA-C4104", detail: "export-result" })?,
                ),
            ));
        }
    }
    zryna_abi::verify_v1(zryna_abi::raw::Module::new(exports)).map_err(|_| DeclarationError {
        code: "ZRYNA-C4104",
        detail: "export-name-or-signature",
    })?;
    Ok(())
}

fn check_status(
    operation: &zryna_syntax::native_c_v0::raw::Operation,
    outputs: &[u8],
) -> Result<(), DeclarationError> {
    if operation.mode == Mode::Status {
        require(
            operation.result == AbiType::CI32 && !operation.statuses.is_empty(),
            "ZRYNA-C4104",
            "status-carrier",
        )?;
        require(
            operation.statuses.windows(2).all(|pair| pair[0].code < pair[1].code),
            "ZRYNA-C4105",
            "status-order",
        )?;
        let success = &operation.statuses[0];
        let owners: Vec<_> = operation
            .resources
            .iter()
            .enumerate()
            .filter_map(|(index, resource)| {
                (resource.access == zryna_syntax::native_c_v0::raw::Access::Create).then_some(index)
            })
            .map(|index| {
                u8::try_from(index)
                    .map_err(|_| DeclarationError { code: "ZRYNA-C4107", detail: "resource-index" })
            })
            .collect::<Result<Vec<_>, _>>()?;
        require(
            success.code == 0
                && success.kind == StatusKind::Success
                && success.condition == Condition::Success
                && success.initialized == outputs
                && success.new_owners == owners,
            "ZRYNA-C4105",
            "success-outputs",
        )?;
        for status in operation.statuses.iter().skip(1) {
            require(
                status.kind == StatusKind::Recoverable
                    && status.condition != Condition::Success
                    && status.initialized.is_empty()
                    && status.new_owners.is_empty(),
                "ZRYNA-C4105",
                "failure-atomicity",
            )?;
        }
        for status in &operation.statuses {
            require(status.preserves_inputs, "ZRYNA-C4105", "input-preservation")?;
            match status.condition {
                Condition::NegativeFirstI32 => require(
                    operation.parameters.first().is_some_and(|parameter| {
                        matches!(parameter.abi, AbiType::CI32 | AbiType::CInt)
                    }),
                    "ZRYNA-C4105",
                    "negative-status-input",
                )?,
                Condition::AllocationFailure | Condition::RawOverLimitOrAllocation => {
                    require(!owners.is_empty(), "ZRYNA-C4105", "allocation-status-owner")?;
                }
                _ => {}
            }
            if matches!(
                status.condition,
                Condition::RawOverLimit | Condition::RawOverLimitOrAllocation
            ) {
                require(
                    operation.resources.iter().any(|resource| resource.kind == "borrowed-bytes"),
                    "ZRYNA-C4105",
                    "length-status-input",
                )?;
            }
        }
    } else {
        require(
            operation.statuses.is_empty()
                && outputs.is_empty()
                && ((operation.mode == Mode::Void) == (operation.result == AbiType::Unit)),
            "ZRYNA-C4104",
            "direct-or-void",
        )?;
    }
    Ok(())
}
