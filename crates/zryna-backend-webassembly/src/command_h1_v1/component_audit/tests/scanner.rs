//! Scanner grammar and count rejection; malformed inputs make no component-validity claim.

use wasm_encoder::{
    CanonicalFunctionSection, CanonicalOption, ComponentExportKind, ComponentExternName,
    ComponentInstanceSection,
};

use super::super::graph_budget;
use super::payload;

#[test]
fn from_exports_uses_external_name_kind_index_without_optional_type_byte() {
    let mut instance = ComponentInstanceSection::new();
    instance.export_items([("run", ComponentExportKind::Func, 1)]);
    let bytes = payload(&instance);
    assert_eq!(bytes, [1, 1, 1, 0, 3, b'r', b'u', b'n', 1, 1]);
    graph_budget::component_instance(&bytes, 0).expect("correct FromExports grammar");
    let mut dangling = bytes.clone();
    dangling.push(0);
    assert!(graph_budget::component_instance(&dangling, 0).is_err());
    assert!(graph_budget::component_instance(&bytes[..bytes.len() - 1], 0).is_err());
    graph_budget::component_instance(&bytes, 0).expect("next correct scanner input recovers");
}

#[test]
fn scanner_rejects_extra_records_exports_and_identity_options_before_allocation() {
    for bytes in [
        vec![2],
        vec![1, 0],
        vec![1, 1, 0],
        vec![1, 1, 2],
        vec![1, 1, 0xff, 0xff, 0xff, 0xff, 0x0f],
    ] {
        assert!(graph_budget::component_instance(&bytes, 0).is_err());
    }
    for (length, accepted) in [(256, true), (257, false)] {
        let name = "x".repeat(length);
        let mut instance = ComponentInstanceSection::new();
        instance.export_items([(name.as_str(), ComponentExportKind::Func, 1)]);
        assert_eq!(graph_budget::component_instance(&payload(&instance), 0).is_ok(), accepted);
    }
    let name = ComponentExternName {
        name: "run".into(),
        implements: Some("run".into()),
        version_suffix: None,
        external_id: None,
    };
    let mut instance = ComponentInstanceSection::new();
    instance.export_items([(name, ComponentExportKind::Func, 1)]);
    let error = graph_budget::component_instance(&payload(&instance), 0)
        .expect_err("unreviewed external-name options reject");
    assert_eq!(error.code(), "ZRYNA-W4013");
}

#[test]
fn canonical_scanner_handles_lower_and_lift_tails_and_rejects_malformed_counts() {
    let mut lower = CanonicalFunctionSection::new();
    lower
        .lower(0, [CanonicalOption::UTF8, CanonicalOption::Memory(0), CanonicalOption::Realloc(0)]);
    let bytes = payload(&lower);
    graph_budget::canonical(&bytes, 0).expect("lower has no trailing type reference");
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(graph_budget::canonical(&extra, 0).is_err());
    let mut lift = CanonicalFunctionSection::new();
    lift.lift(2, 42, []);
    let bytes = payload(&lift);
    graph_budget::canonical(&bytes, 0).expect("lift consumes its required type reference");
    assert!(graph_budget::canonical(&bytes[..bytes.len() - 1], 0).is_err());
    for bytes in [vec![2], vec![1, 1, 0, 0, 4], vec![1, 1, 1, 0, 0]] {
        assert!(graph_budget::canonical(&bytes, 0).is_err());
    }
    assert!(graph_budget::instances(&[4], 0).is_err());
    graph_budget::canonical(&bytes, 0).expect("canonical scanner recovers after malformed inputs");
}
