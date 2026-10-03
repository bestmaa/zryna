//! Synthetic complete raw graphs do not claim source-admissible programs.

use std::collections::BTreeMap;

use super::{Failure, raw, tests, verify};
use crate::{StorageTarget, raw::TypeKind as Base};

fn base(sources: &zryna_source::SourceMap) -> raw::Graph {
    let mut graph = tests::graph(sources, raw::TypeKind::Option { argument: raw::NodeId(1) });
    graph.types.pop();
    graph.program_roots.clear();
    graph
}

fn add(graph: &mut raw::Graph, kind: raw::TypeKind) -> raw::NodeId {
    let id = raw::NodeId(u32::try_from(graph.types.len()).expect("synthetic bounded node count"));
    graph.types.push(raw::TypeNode { id, span: None, kind });
    id
}

fn code(result: Result<super::VerifiedLayouts, Failure>, expected: &str) {
    let Err(Failure::Diagnostics(errors)) = result else {
        panic!("atomic raw graph rejection");
    };
    assert!(errors.iter().any(|error| error.code() == expected), "{errors:?}");
}

#[test]
fn synthetic_closed_type_count_exact_first_extra_and_replay() {
    let sources = tests::sources();
    let mut graph = base(&sources);
    for length in 1..=crate::MAX_TYPE_NODES - 3 {
        let id = add(
            &mut graph,
            raw::TypeKind::Base(Base::FixedArray {
                element: raw::NodeId(1),
                length: u64::try_from(length).expect("array length"),
            }),
        );
        graph.program_roots.push(id);
    }
    let exact =
        verify(&graph, &sources, StorageTarget::Linear32V1).expect("exact complete type count");
    assert_eq!(exact.types().len(), crate::MAX_TYPE_NODES);
    let id = add(
        &mut graph,
        raw::TypeKind::Base(Base::FixedArray { element: raw::NodeId(1), length: 65_534 }),
    );
    graph.program_roots.push(id);
    code(verify(&graph, &sources, StorageTarget::Linear32V1), "ZRYNA-L7201");
    graph.types.pop();
    graph.program_roots.pop();
    assert_eq!(
        verify(&graph, &sources, StorageTarget::Linear32V1).expect("pristine replay").fingerprint(),
        exact.fingerprint()
    );
}

#[test]
fn synthetic_closed_generic_data_count_exact_first_extra_and_duplicate_key_rejection() {
    let sources = tests::sources();
    let mut graph = base(&sources);
    for length in 1..=4096 {
        let argument = add(
            &mut graph,
            raw::TypeKind::Base(Base::FixedArray { element: raw::NodeId(1), length }),
        );
        let id = add(&mut graph, raw::TypeKind::Option { argument });
        graph.program_roots.push(id);
    }
    let exact = verify(&graph, &sources, StorageTarget::LinuxX8664V1).expect("exact generic count");
    assert_eq!(exact.types().filter(|ty| ty.key()[0] == 0x14).count(), 4096);
    let argument = add(
        &mut graph,
        raw::TypeKind::Base(Base::FixedArray { element: raw::NodeId(1), length: 4097 }),
    );
    let id = add(&mut graph, raw::TypeKind::Option { argument });
    graph.program_roots.push(id);
    code(verify(&graph, &sources, StorageTarget::LinuxX8664V1), "ZRYNA-L7201");
    graph.types.pop();
    graph.types.pop();
    graph.program_roots.pop();
    let id = add(&mut graph, raw::TypeKind::Option { argument: raw::NodeId(3) });
    graph.program_roots.push(id);
    code(verify(&graph, &sources, StorageTarget::LinuxX8664V1), "ZRYNA-L3001");
    graph.types.pop();
    graph.program_roots.pop();
    assert_eq!(
        verify(&graph, &sources, StorageTarget::LinuxX8664V1)
            .expect("recovered exact count")
            .fingerprint(),
        exact.fingerprint()
    );
}

#[test]
fn synthetic_complete_key_depth_64_and_65_are_independent_of_by_value_graph_depth() {
    let sources = tests::sources();
    let mut graph = base(&sources);
    let mut child = raw::NodeId(1);
    for _ in 0..64 {
        child = add(&mut graph, raw::TypeKind::Option { argument: child });
    }
    graph.program_roots.push(child);
    assert!(verify(&graph, &sources, StorageTarget::Linear32V1).is_ok());
    let extra = add(&mut graph, raw::TypeKind::Option { argument: child });
    graph.program_roots = vec![extra];
    code(verify(&graph, &sources, StorageTarget::Linear32V1), "ZRYNA-L7201");
    graph.types.pop();
    graph.program_roots = vec![child];
    assert!(verify(&graph, &sources, StorageTarget::Linear32V1).is_ok());
}

fn tree(
    graph: &mut raw::Graph,
    leaves: usize,
    cache: &mut BTreeMap<usize, raw::NodeId>,
) -> raw::NodeId {
    if leaves == 1 {
        return raw::NodeId(1);
    }
    if let Some(id) = cache.get(&leaves) {
        return *id;
    }
    let okay = tree(graph, leaves / 2, cache);
    let error = tree(graph, leaves - leaves / 2, cache);
    let id = add(graph, raw::TypeKind::Result { okay, error });
    cache.insert(leaves, id);
    id
}

#[test]
fn synthetic_complete_valid_key_bytes_4096_and_4097_use_finite_shared_argument_trees() {
    let sources = tests::sources();
    for (wrappers, nominal, expected) in [(2, true, 4096), (4, false, 4097)] {
        let mut graph = base(&sources);
        let mut child = tree(&mut graph, 291, &mut BTreeMap::new());
        if nominal {
            let file = sources.verify_file_id(0).expect("source");
            let span = sources.span(file, 0, 63).expect("original span");
            graph.modules[0].data_declarations = 1;
            graph.declarations.push(raw::Declaration {
                module: raw::ModuleId(0),
                index: 0,
                kind: raw::NominalKind::Struct,
                parameters: 1,
                members: 1,
                span,
            });
            let id = add(
                &mut graph,
                raw::TypeKind::Struct {
                    module: raw::ModuleId(0),
                    declaration: 0,
                    arguments: vec![child],
                    fields: vec![raw::Field { ordinal: 0, ty: child }],
                },
            );
            graph.types[id.0 as usize].span = Some(span);
            child = id;
        }
        for _ in 0..wrappers {
            child = add(&mut graph, raw::TypeKind::Option { argument: child });
        }
        graph.program_roots = vec![child];
        // Result trees with n scalar leaves have 14*n-13 bytes. Box adds 17; Option adds 9.
        assert_eq!(14 * 291 - 13 + usize::from(nominal) * 17 + wrappers * 9, expected);
        let result = verify(&graph, &sources, StorageTarget::Linear32V1);
        if expected == 4096 {
            assert!(result.expect("exact complete key").types().any(|ty| ty.key().len() == 4096));
        } else {
            code(result, "ZRYNA-L7201");
        }
    }
}

#[test]
fn synthetic_checked_object_arithmetic_rejects_before_any_target_seal_and_recovers() {
    let sources = tests::sources();
    let mut graph = base(&sources);
    let inner = add(
        &mut graph,
        raw::TypeKind::Base(Base::FixedArray {
            element: raw::NodeId(1),
            length: crate::MAX_ARRAY_LENGTH,
        }),
    );
    let outer = add(
        &mut graph,
        raw::TypeKind::Base(Base::FixedArray { element: inner, length: crate::MAX_ARRAY_LENGTH }),
    );
    graph.program_roots = vec![outer];
    code(verify(&graph, &sources, StorageTarget::Linear32V1), "ZRYNA-L3005");
    graph.types.pop();
    graph.program_roots = vec![inner];
    assert!(verify(&graph, &sources, StorageTarget::Linear32V1).is_ok());
}

#[test]
fn synthetic_original_member_exact_first_extra_and_aggregate_next_instance_rejection() {
    let sources = tests::sources();
    let kind = raw::TypeKind::Struct {
        module: raw::ModuleId(0),
        declaration: 0,
        arguments: vec![raw::NodeId(1)],
        fields: (0..1024).map(|ordinal| raw::Field { ordinal, ty: raw::NodeId(1) }).collect(),
    };
    let mut graph = tests::graph(&sources, kind);
    graph.declarations[0].members = 1024;
    assert!(verify(&graph, &sources, StorageTarget::Linear32V1).is_ok());
    graph.declarations[0].members = 1025;
    if let raw::TypeKind::Struct { fields, .. } = &mut graph.types[3].kind {
        fields.push(raw::Field { ordinal: 1024, ty: raw::NodeId(1) });
    }
    code(verify(&graph, &sources, StorageTarget::Linear32V1), "ZRYNA-L7201");
    graph.declarations[0].members = 1024;
    if let raw::TypeKind::Struct { fields, .. } = &mut graph.types[3].kind {
        fields.pop();
    }
    for length in 1..64 {
        let argument = add(
            &mut graph,
            raw::TypeKind::Base(Base::FixedArray { element: raw::NodeId(1), length }),
        );
        let mut instance = graph.types[3].clone();
        instance.id = raw::NodeId(u32::try_from(graph.types.len()).expect("instance count"));
        if let raw::TypeKind::Struct { arguments, .. } = &mut instance.kind {
            arguments[0] = argument;
        }
        graph.types.push(instance);
    }
    assert!(verify(&graph, &sources, StorageTarget::Linear32V1).is_ok());
    let argument = add(
        &mut graph,
        raw::TypeKind::Base(Base::FixedArray { element: raw::NodeId(1), length: 64 }),
    );
    let mut extra = graph.types[3].clone();
    extra.id = raw::NodeId(u32::try_from(graph.types.len()).expect("first extra instance"));
    if let raw::TypeKind::Struct { arguments, .. } = &mut extra.kind {
        arguments[0] = argument;
    }
    graph.types.push(extra);
    code(verify(&graph, &sources, StorageTarget::Linear32V1), "ZRYNA-L7201");
}
