//! Independent mutations keep the actual source/material/body/private issuer fixed.
#[allow(dead_code)]
mod capture;
#[path = "hostile/resources.rs"]
mod resources;

use zryna_native_c_ir::{lower, lower_unverified, raw, verify};
use zryna_semantics::native_c_v0::body::FlowStep;
use zryna_syntax::native_c_v0::raw::{AbiType, Safety};

fn reject(change: impl FnOnce(&mut raw::Program), code: &str) {
    let input = capture::reference();
    let mut candidate =
        lower_unverified(&input.sources, &input.authority).expect("untrusted candidate");
    change(&mut candidate);
    let error = verify(candidate, &input.sources, &input.authority)
        .expect_err("hostile independent candidate must reject without a seal");
    assert_eq!(error.code(), code, "{error}");
    if let Some(span) = error.span() {
        assert!(input.sources.resolve(span).is_ok());
    }
    assert!(
        lower(&input.sources, &input.authority).is_ok(),
        "rejection cannot poison the genuine issuer"
    );
}

#[test]
fn rebuilt_source_and_other_real_issuer_cannot_replace_original_authority() {
    let input = capture::reference();
    let other = capture::reference();
    let candidate = lower_unverified(&input.sources, &input.authority).expect("claims");
    let error = verify(candidate, &other.sources, &input.authority)
        .expect_err("equal bytes are another map");
    assert_eq!(error.code(), "ZRYNA-C4106");
    assert!(error.span().is_none());
    let candidate =
        lower_unverified(&other.sources, &other.authority).expect("other genuine claims");
    assert_eq!(
        verify(candidate, &input.sources, &input.authority).expect_err("another issuer").code(),
        "ZRYNA-C4102"
    );
}

#[test]
fn missing_duplicate_and_reordered_original_occupants_fail_closed() {
    reject(
        |p| {
            p.functions.pop();
        },
        "ZRYNA-C4106",
    );
    reject(
        |p| {
            p.functions[1] = p.functions[0].clone();
        },
        "ZRYNA-C4106",
    );
    reject(
        |p| {
            p.functions[0].values[1].id = 0;
        },
        "ZRYNA-C4106",
    );
    reject(
        |p| {
            p.functions[0].statements.swap(0, 1);
        },
        "ZRYNA-C4106",
    );
}

#[test]
fn changed_add_operand_binding_and_literal_do_not_match_source() {
    reject(
        |p| {
            let value = p
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.values)
                .find(|v| matches!(v.kind, raw::ValueKind::WrappingAdd(_, _)))
                .expect("wrapping add");
            if let raw::ValueKind::WrappingAdd(left, right) = &mut value.kind {
                *right = *left;
            }
        },
        "ZRYNA-C4106",
    );
    reject(
        |p| {
            let value = p.functions[0]
                .values
                .iter_mut()
                .find(|v| matches!(v.kind, raw::ValueKind::Local(_)))
                .expect("local");
            value.kind = raw::ValueKind::Local(999);
        },
        "ZRYNA-C4106",
    );
    let input = capture::edited(
        capture::BUFFER,
        capture::HANDLE,
        &format!("{}\nfunction constant(): i32 {{ return 7; }}\n", capture::SCALAR),
    );
    let mut p = lower_unverified(&input.sources, &input.authority).expect("genuine literal");
    p.functions.iter_mut().find(|f| f.name == "constant").expect("source function").values[0]
        .kind = raw::ValueKind::I32(8);
    assert_eq!(
        verify(p, &input.sources, &input.authority).expect_err("changed source literal").code(),
        "ZRYNA-C4106"
    );
}

#[test]
fn changed_value_category_and_status_call_provenance_reject() {
    reject(
        |p| {
            p.functions[0].values[0].ty = zryna_semantics::native_c_v0::body::ValueType::Bool;
        },
        "ZRYNA-C4104",
    );
    reject(
        |p| {
            let value = p.functions[0]
                .values
                .iter_mut()
                .find(|v| v.status_call.is_some())
                .expect("status result");
            value.status_call = Some(99);
        },
        "ZRYNA-C4105",
    );
    reject(
        |p| {
            let value = p.functions[0]
                .values
                .iter_mut()
                .find(|v| v.origin.is_some())
                .expect("retained private origin");
            value.origin = Some(zryna_semantics::native_c_v0::body::PrivateOrigin::Copy(99));
        },
        "ZRYNA-C4105",
    );
}

#[test]
fn identity_precedes_target_and_signature_resource_source_categories_remain_distinct() {
    reject(
        |p| {
            let parameter = p
                .declarations
                .operations
                .iter_mut()
                .flat_map(|op| &mut op.parameters)
                .find(|parameter| parameter.resource.is_some())
                .expect("resource role");
            parameter.resource = None;
        },
        "ZRYNA-C4105",
    );
    reject(
        |p| {
            p.declarations.target = "x86_64-pc-windows-msvc".into();
        },
        "ZRYNA-C4103",
    );
    reject(
        |p| {
            p.declarations.target = "x86_64-pc-windows-msvc".into();
            p.declarations.libraries[0].header_sha256 = "0".repeat(64);
        },
        "ZRYNA-C4102",
    );
    reject(
        |p| {
            p.declarations.operations[0].parameters[0].abi = AbiType::CInt;
        },
        "ZRYNA-C4104",
    );
    reject(
        |p| {
            p.declarations.operations[0].symbol = "renamed".into();
        },
        "ZRYNA-C4102",
    );
    reject(
        |p| {
            p.declarations
                .operations
                .iter_mut()
                .find(|o| !o.resources.is_empty())
                .expect("resource")
                .resources[0]
                .release = "wrong@0/free".into();
        },
        "ZRYNA-C4105",
    );
    reject(
        |p| {
            p.declarations.sites[0].safety = Safety::UnsafeRaw;
        },
        "ZRYNA-C4106",
    );
}

#[test]
fn no_unused_declaration_or_library_may_disappear_from_complete_accounting() {
    reject(
        |p| {
            p.declarations.operations.pop();
        },
        "ZRYNA-C4102",
    );
    reject(
        |p| {
            p.declarations.libraries.clear();
        },
        "ZRYNA-C4102",
    );
}

#[test]
fn raw_safe_marker_and_exact_call_carriers_cannot_be_changed() {
    reject(
        |p| {
            if let FlowStep::Call { safety, .. } = &mut p.functions[0]
                .effects
                .iter_mut()
                .find(|e| matches!(e.operation, FlowStep::Call { .. }))
                .expect("call")
                .operation
            {
                *safety = Safety::Safe;
            }
        },
        "ZRYNA-C4106",
    );
    reject(
        |p| {
            if let FlowStep::Call { carriers, .. } = &mut p.functions[0]
                .effects
                .iter_mut()
                .find(|e| matches!(e.operation, FlowStep::Call { .. }))
                .expect("call")
                .operation
            {
                carriers[0] = AbiType::Bool32;
            }
        },
        "ZRYNA-C4104",
    );
}
