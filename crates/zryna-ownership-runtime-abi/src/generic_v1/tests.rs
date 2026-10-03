//! Independent hostile declarations and source-brand replay controls.

use super::*;
use zryna_layout::{
    generic_v1::{raw as layout, verify},
    raw::TypeKind as Base,
};
use zryna_source::{SourceFileInput, SourceMap};

fn source() -> SourceMap {
    SourceMap::build(vec![SourceFileInput {
        path: "main.zry".into(),
        text: "// generic runtime metadata".into(),
    }])
    .expect("genuine fixture invariant")
}

fn graph(sources: &SourceMap) -> layout::Graph {
    let kinds = vec![
        layout::TypeKind::Base(Base::Bool),
        layout::TypeKind::Base(Base::I32),
        layout::TypeKind::Base(Base::String),
        layout::TypeKind::Option { argument: layout::NodeId(1) },
        layout::TypeKind::Base(Base::Vec { element: layout::NodeId(3) }),
        layout::TypeKind::Base(Base::Shared { payload: layout::NodeId(3) }),
        layout::TypeKind::Base(Base::Weak { payload: layout::NodeId(3) }),
        layout::TypeKind::Result { okay: layout::NodeId(1), error: layout::NodeId(2) },
    ];
    layout::Graph {
        modules: vec![layout::Module {
            id: layout::ModuleId(0),
            source_file: sources.verify_file_id(0).expect("genuine fixture invariant"),
            data_declarations: 0,
        }],
        declarations: vec![],
        types: kinds
            .into_iter()
            .enumerate()
            .map(|(index, kind)| layout::TypeNode {
                id: layout::NodeId(u32::try_from(index).expect("genuine fixture invariant")),
                span: None,
                kind,
            })
            .collect(),
        program_roots: (0..8).map(layout::NodeId).collect(),
    }
}

fn layouts(sources: &SourceMap) -> (VerifiedLayouts, VerifiedLayouts) {
    let graph = graph(sources);
    (
        verify(&graph, sources, StorageTarget::Linear32V1).expect("genuine fixture invariant"),
        verify(&graph, sources, StorageTarget::LinuxX8664V1).expect("genuine fixture invariant"),
    )
}

#[test]
fn closed_option_controls_and_vec_strides_are_independently_derived() {
    let sources = source();
    let (linear, linux) = layouts(&sources);
    let abi =
        verify_v1(raw_v1(&linear, &linux).expect("genuine fixture invariant"), &linear, &linux)
            .expect("genuine fixture invariant");
    assert_eq!(abi.declarations().identifier, IDENTIFIER);
    assert_eq!(abi.operations().len(), crate::OPERATIONS.len());
    assert_eq!(abi.element_layouts().len(), 2);
    assert_eq!(abi.control_layouts().len(), 2); // Shared and Weak deduplicate one payload per target.
    for element in abi.element_layouts() {
        assert_eq!(element.stride(), 8);
        assert_eq!(element.alignment(), 4);
        assert_eq!(
            abi.layouts(element.target())
                .type_by_id(element.element())
                .expect("genuine fixture invariant")
                .key()[0],
            0x14
        );
    }
    for control in abi.control_layouts() {
        assert_eq!((control.payload_offset(), control.size(), control.alignment()), (8, 16, 4));
    }
    assert!(abi.is_bound_to(&linear, &linux));
}

#[test]
fn exact_same_text_other_compilation_rejects_all_branded_replay() {
    let first = source();
    let second = source();
    let (a, b) = layouts(&first);
    let (c, d) = layouts(&second);
    assert_eq!(a.fingerprint(), c.fingerprint());
    assert!(raw_v1(&a, &d).is_err());
    assert!(raw_v1(&b, &a).is_err());
    let left = verify_v1(raw_v1(&a, &b).expect("genuine fixture invariant"), &a, &b)
        .expect("genuine fixture invariant");
    let right = verify_v1(raw_v1(&c, &d).expect("genuine fixture invariant"), &c, &d)
        .expect("genuine fixture invariant");
    assert_eq!(left.identity().fingerprint(), right.identity().fingerprint());
    assert_ne!(left.identity(), right.identity());
    assert!(!left.is_bound_to(&c, &d));
    assert!(
        right
            .operation(left.operations().next().expect("genuine fixture invariant").id())
            .is_none()
    );
    assert!(c.type_by_id(left.element_layouts()[0].element()).is_none());
    assert!(
        left.operation(left.operations().next().expect("genuine fixture invariant").id()).is_some()
    );
}

#[test]
fn every_mutated_declaration_category_and_older_domain_is_rejected() {
    let sources = source();
    let (linear, linux) = layouts(&sources);
    let exact = raw_v1(&linear, &linux).expect("genuine fixture invariant");
    let mut changed = vec![];
    let mut add = |edit: fn(&mut raw::Contract)| {
        let mut claim = exact.clone();
        edit(&mut claim);
        changed.push(claim);
    };
    add(|claim| claim.identifier = crate::OWNERSHIP_RUNTIME_V1_IDENTIFIER.into());
    add(|claim| claim.schema_version += 1);
    add(|claim| claim.layout_claims[0].fingerprint[0] ^= 1);
    add(|claim| claim.statuses[0].numeric = 7);
    add(|claim| claim.operations[0].parameters.clear());
    add(|claim| claim.javascript[0].operation = "foreign".into());
    add(|claim| claim.webassembly[0].results.clear());
    add(|claim| claim.native_linux_x86_64[0].symbol = "foreign".into());
    add(|claim| {
        claim
            .records
            .last_mut()
            .expect("genuine fixture invariant")
            .fields
            .last_mut()
            .expect("genuine fixture invariant")
            .offset += 1;
    });
    add(|claim| claim.native_header[0] ^= 1);
    add(|claim| claim.records.reverse());
    for claim in changed {
        assert!(verify_v1(claim, &linear, &linux).is_err());
    }
}

#[test]
fn input_budget_rejects_before_declaration_comparison() {
    let sources = source();
    let (linear, linux) = layouts(&sources);
    let mut claim = raw_v1(&linear, &linux).expect("genuine fixture invariant");
    claim.operations.resize(crate::MAX_RUNTIME_OPERATIONS + 1, claim.operations[0].clone());
    let Failure::Violations(errors) =
        verify_v1(claim, &linear, &linux).expect_err("hostile claim must reject")
    else {
        panic!("wrong failure")
    };
    assert_eq!(errors[0].kind, RuntimeAbiViolationKind::Budget);
}
