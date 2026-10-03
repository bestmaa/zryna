//! Canonical fixed declaration inventory shared by separately branded issuers.

use super::{
    CHECKED_NATIVE_HEADER, OPERATIONS, OWNERSHIP_RUNTIME_V1_IDENTIFIER,
    OWNERSHIP_RUNTIME_V1_SCHEMA_VERSION, VerifiedLayouts, canonical_records, native_declaration,
    operation_declaration, raw, status, wasm_declaration,
};

pub(crate) fn canonical_contract(
    linear: &VerifiedLayouts,
    linux: &VerifiedLayouts,
) -> raw::Contract {
    fixed_contract(
        vec![
            raw::LayoutClaim {
                target: raw::LayoutTarget::Linear32V1,
                universe: linear.universe_identity().as_bytes(),
                fingerprint: *linear.fingerprint(),
            },
            raw::LayoutClaim {
                target: raw::LayoutTarget::LinuxX8664V1,
                universe: linux.universe_identity().as_bytes(),
                fingerprint: *linux.fingerprint(),
            },
        ],
        canonical_records(linear, linux),
    )
}

pub(crate) fn fixed_contract(
    layout_claims: Vec<raw::LayoutClaim>,
    records: Vec<raw::RecordDeclaration>,
) -> raw::Contract {
    let operations = OPERATIONS.iter().copied().map(operation_declaration).collect::<Vec<_>>();
    let javascript = operations
        .iter()
        .map(|operation| raw::JavaScriptHelper {
            operation: operation.name.clone(),
            parameters: operation.parameters.clone(),
            result: match operation.result {
                raw::LogicalResult::Status => raw::JavaScriptResultShape::Status,
                raw::LogicalResult::StatusPointer => raw::JavaScriptResultShape::StatusPointer,
                raw::LogicalResult::StatusString | raw::LogicalResult::StatusVecStorage => {
                    raw::JavaScriptResultShape::StatusHandle
                }
                raw::LogicalResult::StatusBool => raw::JavaScriptResultShape::StatusBool,
            },
        })
        .collect();
    raw::Contract {
        schema_version: OWNERSHIP_RUNTIME_V1_SCHEMA_VERSION,
        identifier: OWNERSHIP_RUNTIME_V1_IDENTIFIER.to_owned(),
        layout_claims,
        statuses: vec![
            status(0, "OK", raw::StatusDisposition::Success, None),
            status(
                1,
                "ALLOCATION",
                raw::StatusDisposition::ControlledTrap,
                Some("zryna.trap.allocation-v1"),
            ),
            status(
                2,
                "CAPACITY",
                raw::StatusDisposition::ControlledTrap,
                Some("zryna.trap.capacity-v1"),
            ),
            status(
                3,
                "REFCOUNT",
                raw::StatusDisposition::ControlledTrap,
                Some("zryna.trap.refcount-v1"),
            ),
            status(4, "UTF8", raw::StatusDisposition::ControlledTrap, Some("zryna.trap.utf8-v1")),
            status(5, "EXPIRED", raw::StatusDisposition::Branch, None),
            status(255, "ABI_VIOLATION", raw::StatusDisposition::HostFailure, None),
        ],
        operations,
        javascript,
        webassembly: OPERATIONS.iter().copied().map(wasm_declaration).collect(),
        native_linux_x86_64: OPERATIONS.iter().copied().map(native_declaration).collect(),
        records,
        native_header: CHECKED_NATIVE_HEADER.to_vec(),
    }
}
