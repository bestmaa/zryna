//! Raw successor layout checks are independent of semantic source admission.

use std::fmt::Write as _;

use super::{Failure, raw, verify, verify_claim};
use crate::{StorageTarget, raw::TypeKind as Base};
use serde_json::Value;
use sha2::{Digest, Sha256};
use zryna_source::{SourceFileInput, SourceMap};

pub(super) fn sources() -> SourceMap {
    SourceMap::build(vec![SourceFileInput {
        path: "main.zry".into(),
        text: "interface Box<T extends ZrynaValue> extends ZrynaStruct {value:T;}".into(),
    }])
    .expect("immutable source map")
}

pub(super) fn graph(sources: &SourceMap, kind: raw::TypeKind) -> raw::Graph {
    let file = sources.verify_file_id(0).expect("fixture file");
    let nominal = match kind {
        raw::TypeKind::Struct { .. } => Some((raw::NominalKind::Struct, 1)),
        raw::TypeKind::Enum { .. } => Some((raw::NominalKind::Enum, 2)),
        _ => None,
    };
    let span = sources.span(file, 0, 63).expect("original declaration span");
    let declarations = nominal.map_or_else(Vec::new, |(kind, parameters)| {
        vec![raw::Declaration {
            module: raw::ModuleId(0),
            index: 0,
            kind,
            parameters,
            members: parameters,
            span,
        }]
    });
    let mut types = [Base::Bool, Base::I32, Base::String]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| raw::TypeNode {
            id: raw::NodeId(u32::try_from(index).expect("primitive index")),
            span: None,
            kind: raw::TypeKind::Base(kind),
        })
        .collect::<Vec<_>>();
    types.push(raw::TypeNode { id: raw::NodeId(3), span: nominal.map(|_| span), kind });
    raw::Graph {
        modules: vec![raw::Module {
            id: raw::ModuleId(0),
            source_file: file,
            data_declarations: u32::try_from(declarations.len()).expect("declarations"),
        }],
        declarations,
        types,
        program_roots: vec![raw::NodeId(3)],
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::new();
    for byte in bytes {
        write!(text, "{byte:02x}").expect("fixture hex");
    }
    text
}

fn last_record(bytes: &[u8]) -> &[u8] {
    let mut cursor = b"ZRYNA-GENERIC-AGGREGATE-LAYOUT-V1\0".len() + 8;
    let mut start = cursor;
    while cursor < bytes.len() {
        start = cursor;
        let length =
            u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().expect("record length"));
        cursor += 4 + usize::try_from(length).expect("record bytes");
    }
    &bytes[start..]
}

fn fixture_kind(name: &str) -> raw::TypeKind {
    match name {
        "Option<i32>" => raw::TypeKind::Option { argument: raw::NodeId(1) },
        "Result<i32,bool>" => raw::TypeKind::Result { okay: raw::NodeId(1), error: raw::NodeId(0) },
        "Box<i32>" => raw::TypeKind::Struct {
            module: raw::ModuleId(0),
            declaration: 0,
            arguments: vec![raw::NodeId(1)],
            fields: vec![raw::Field { ordinal: 0, ty: raw::NodeId(1) }],
        },
        "Choice<i32,bool>" => raw::TypeKind::Enum {
            module: raw::ModuleId(0),
            declaration: 0,
            arguments: vec![raw::NodeId(1), raw::NodeId(0)],
            variants: vec![
                raw::Variant { ordinal: 0, payload: Some(raw::NodeId(1)) },
                raw::Variant { ordinal: 1, payload: Some(raw::NodeId(0)) },
            ],
        },
        _ => panic!("unexpected independently frozen fixture"),
    }
}

#[test]
fn successor_four_fixed_records_keys_and_dual_target_digests_match_independent_oracles() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../../../spec/memory-model/generic-enum-layout-v1-fixtures.json"
    ))
    .expect("fixed reference JSON");
    let sources = sources();
    for case in fixtures["cases"].as_array().expect("fixed cases") {
        let graph = graph(&sources, fixture_kind(case["name"].as_str().expect("fixture name")));
        for (target, prefix) in
            [(StorageTarget::Linear32V1, "linear32"), (StorageTarget::LinuxX8664V1, "linuxX8664")]
        {
            let layouts = verify(&graph, &sources, target).expect("independent successor layout");
            let ty = layouts.types().last().expect("closed family record");
            assert_eq!(hex(ty.key()), case["typeKeyHex"].as_str().expect("fixed complete key"));
            assert_eq!(
                hex(&Sha256::digest(ty.key())),
                case["typeKeySha256"].as_str().expect("fixed key hash")
            );
            assert_eq!(
                hex(last_record(layouts.canonical_bytes())),
                case["recordHex"].as_str().expect("fixed record")
            );
            assert_eq!(
                layouts.canonical_bytes().len() as u64,
                case[format!("{prefix}DocumentBytes")].as_u64().expect("document size")
            );
            assert_eq!(
                hex(layouts.fingerprint()),
                case[format!("{prefix}Sha256")].as_str().expect("fixed target digest")
            );
            assert!(
                verify_claim(
                    &graph,
                    &sources,
                    target,
                    layouts.canonical_bytes(),
                    layouts.fingerprint()
                )
                .is_ok()
            );
        }
    }
}

#[test]
fn owned_nested_standard_enum_layouts_match_every_fixed_prefix_on_both_targets() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../../../spec/memory-model/generic-owned-layout-v1-fixtures.json"
    ))
    .expect("owned fixed reference JSON");
    let sources = sources();
    let mut graph = graph(&sources, raw::TypeKind::Option { argument: raw::NodeId(2) });
    for (index, case) in fixtures["cases"].as_array().expect("owned cases").iter().enumerate() {
        if index == 1 {
            graph.types.push(raw::TypeNode {
                id: raw::NodeId(4),
                span: None,
                kind: raw::TypeKind::Option { argument: raw::NodeId(3) },
            });
        }
        if index == 2 {
            graph.types.push(raw::TypeNode {
                id: raw::NodeId(5),
                span: None,
                kind: raw::TypeKind::Result { okay: raw::NodeId(3), error: raw::NodeId(2) },
            });
        }
        graph.program_roots =
            (3..u32::try_from(graph.types.len()).expect("prefix size")).map(raw::NodeId).collect();
        for expected in case["targets"].as_array().expect("both targets") {
            let target = if expected["target"] == 1 {
                StorageTarget::Linear32V1
            } else {
                StorageTarget::LinuxX8664V1
            };
            let layouts = verify(&graph, &sources, target).expect("owned successor layout");
            let ty = layouts.types().last().expect("last closed prefix member");
            assert_eq!(ty.size(), expected["size"].as_u64().expect("stored size"));
            assert_eq!(ty.alignment(), expected["alignment"].as_u64().expect("alignment"));
            assert_eq!(ty.drop_kind(), 1);
            assert_eq!(ty.runtime_kind(), 1);
            assert_eq!(
                hex(last_record(layouts.canonical_bytes())),
                expected["recordHex"].as_str().expect("fixed record")
            );
            assert_eq!(
                hex(layouts.fingerprint()),
                expected["sha256"].as_str().expect("fixed full prefix hash")
            );
        }
    }
}

#[test]
fn independent_record_mutations_and_wrong_target_fail_before_any_successor_seal() {
    let sources = sources();
    let graph = graph(&sources, fixture_kind("Option<i32>"));
    let layouts = verify(&graph, &sources, StorageTarget::Linear32V1).expect("complete layout");
    for index in 0..layouts.canonical_bytes().len() {
        let mut bytes = layouts.canonical_bytes().to_vec();
        bytes[index] ^= 1;
        assert!(
            verify_claim(
                &graph,
                &sources,
                StorageTarget::Linear32V1,
                &bytes,
                layouts.fingerprint()
            )
            .is_err(),
            "mutation byte {index}"
        );
    }
    let mut fingerprint = *layouts.fingerprint();
    fingerprint[0] ^= 1;
    assert!(
        verify_claim(
            &graph,
            &sources,
            StorageTarget::Linear32V1,
            layouts.canonical_bytes(),
            &fingerprint
        )
        .is_err()
    );
    assert!(
        verify_claim(
            &graph,
            &sources,
            StorageTarget::LinuxX8664V1,
            layouts.canonical_bytes(),
            layouts.fingerprint()
        )
        .is_err()
    );
}

#[test]
fn foreign_equal_text_source_authorities_and_cross_universe_type_ids_reject() {
    let first = sources();
    let second = sources();
    let graph = graph(&first, fixture_kind("Box<i32>"));
    assert!(verify(&graph, &second, StorageTarget::Linear32V1).is_err());
    let a = verify(&graph, &first, StorageTarget::Linear32V1).expect("first source layout");
    let b = verify(
        &super::tests::graph(&second, fixture_kind("Box<i32>")),
        &second,
        StorageTarget::Linear32V1,
    )
    .expect("second source layout");
    let id = a.types().last().expect("first exact ID").id();
    assert!(b.type_by_id(id).is_none());
    let other = super::tests::graph(&first, fixture_kind("Result<i32,bool>"));
    assert!(
        verify(&other, &first, StorageTarget::Linear32V1)
            .expect("different universe")
            .type_by_id(id)
            .is_none()
    );
}

#[test]
fn hostile_graph_identity_arity_ordinals_and_payload_references_reject_then_recover() {
    let sources = sources();
    let original = graph(&sources, fixture_kind("Choice<i32,bool>"));
    for mutation in 0..6 {
        let mut graph = original.clone();
        match mutation {
            0 => graph.types[3].id = raw::NodeId(0),
            1 => graph.types[3].span = None,
            2 => graph.declarations[0].parameters = 1,
            3 => {
                if let raw::TypeKind::Enum { arguments, .. } = &mut graph.types[3].kind {
                    arguments[0] = raw::NodeId(999);
                }
            }
            4 => {
                if let raw::TypeKind::Enum { variants, .. } = &mut graph.types[3].kind {
                    variants[0].ordinal = 1;
                }
            }
            _ => {
                if let raw::TypeKind::Enum { variants, .. } = &mut graph.types[3].kind {
                    variants[0].payload = Some(raw::NodeId(999));
                }
            }
        }
        assert!(matches!(
            verify(&graph, &sources, StorageTarget::Linear32V1),
            Err(Failure::Diagnostics(_))
        ));
        assert!(verify(&original, &sources, StorageTarget::Linear32V1).is_ok());
    }
}

#[test]
fn direct_by_value_cycle_rejects_while_nominal_vec_indirection_remains_finite() {
    let sources = sources();
    let mut graph = graph(&sources, fixture_kind("Box<i32>"));
    if let raw::TypeKind::Struct { fields, .. } = &mut graph.types[3].kind {
        fields[0].ty = raw::NodeId(3);
    }
    let Err(Failure::Diagnostics(errors)) = verify(&graph, &sources, StorageTarget::Linear32V1)
    else {
        panic!("layout must reject by-value cycle");
    };
    assert!(errors.iter().any(|error| error.code() == "ZRYNA-L3002"));
    graph.types.push(raw::TypeNode {
        id: raw::NodeId(4),
        span: None,
        kind: raw::TypeKind::Base(Base::Vec { element: raw::NodeId(3) }),
    });
    if let raw::TypeKind::Struct { fields, .. } = &mut graph.types[3].kind {
        fields[0].ty = raw::NodeId(4);
    }
    assert!(verify(&graph, &sources, StorageTarget::Linear32V1).is_ok());
}

#[test]
fn duplicate_program_roots_do_not_change_identity_or_consume_closed_type_budget() {
    let sources = sources();
    let mut graph = graph(&sources, fixture_kind("Option<i32>"));
    let target = StorageTarget::Linear32V1;
    let original = verify(&graph, &sources, target).expect("original graph");
    graph.program_roots = vec![raw::NodeId(3); 65537];
    let repeated = verify(&graph, &sources, target).expect("duplicate occurrences share one root");
    assert_eq!(original.universe_identity(), repeated.universe_identity());
    assert_eq!(original.canonical_bytes(), repeated.canonical_bytes());
}
