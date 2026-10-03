//! Synthetic budget proofs exercise ceilings independently of inherited source limits.

use super::model::{Builder, Node};
use super::*;
use crate::bounded_generics_v1::tests::body_fixtures::project;
use crate::bounded_generics_v1::{SemanticInput, resolve_declarations};
use zryna_source::NormalizedSourcePath;

fn with_builder(test: impl FnOnce(&mut Builder<'_, '_, '_>, DeclarationIdentity)) {
    let input =
        project(&[("main.zry", "function identity<T extends ZrynaValue>(x:T):T {return x;}")]);
    let entry = input
        .sources
        .file_id(&NormalizedSourcePath::new("main.zry").expect("fixture path"))
        .expect("entry");
    let declarations = resolve_declarations(
        SemanticInput::try_new(&input.syntax, &input.sources, entry).expect("original input"),
    )
    .expect("declarations");
    let bodies = super::super::body_types::check_body_types(&declarations).expect("opaque body");
    let owner = declarations
        .modules()
        .next()
        .expect("module")
        .functions()
        .next()
        .expect("identity")
        .identity();
    test(&mut Builder::new(&bodies).expect("builder storage"), owner);
}

fn array(builder: &mut Builder<'_, '_, '_>, length: u32) -> usize {
    let (key, bytes) = keys::encode(0x20, &[length], &[&[1]]).expect("array key");
    assert_eq!(bytes, 10);
    builder
        .intern(
            Node {
                key,
                shape: TypeShape::FixedArray(length),
                arguments: [Some(1), None],
                depth: 1,
                processed: true,
                members: vec![],
                value: true,
            },
            None,
        )
        .expect("synthetic array")
}

fn budget_error(error: InstantiationFailure, metric: &str, extra: usize) {
    let InstantiationFailure::Diagnostics(errors) = error else {
        panic!("source resource diagnostic");
    };
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "ZRYNA-M7201");
    assert!(errors[0].message().contains(metric));
    assert!(errors[0].message().contains(&format!("rejected count {extra}")));
    assert!(errors[0].primary_span().is_none());
}

#[test]
fn function_inventory_exact_duplicate_and_first_extra() {
    with_builder(|builder, owner| {
        for length in 1..=MAX_FUNCTION_INSTANCES {
            let argument = array(builder, u32::try_from(length).expect("bounded length"));
            builder.function(owner, [Some(argument), None], None).expect("exact inventory member");
            builder
                .function(owner, [Some(argument), None], None)
                .expect("duplicate consumes no slot");
        }
        assert_eq!(builder.functions.len(), MAX_FUNCTION_INSTANCES);
        let extra = array(builder, u32::try_from(MAX_FUNCTION_INSTANCES + 1).expect("first extra"));
        let error = builder
            .function(owner, [Some(extra), None], None)
            .expect_err("first extra before inventory publication");
        budget_error(error, "closed generic functions", MAX_FUNCTION_INSTANCES + 1);
        assert_eq!(builder.functions.len(), MAX_FUNCTION_INSTANCES);
    });
}

#[test]
fn data_inventory_exact_duplicate_and_first_extra() {
    with_builder(|builder, _| {
        for length in 1..=MAX_DATA_INSTANCES + 1 {
            let argument = array(builder, u32::try_from(length).expect("bounded length"));
            let child = builder.types[argument].key.clone();
            let (key, _) = keys::encode(0x14, &[1], &[&child]).expect("Option key");
            let node = Node {
                key,
                shape: TypeShape::Option,
                arguments: [Some(argument), None],
                depth: 2,
                processed: true,
                members: vec![],
                value: true,
            };
            if length <= MAX_DATA_INSTANCES {
                builder.intern(node.clone(), None).expect("exact inventory member");
                builder.intern(node, None).expect("duplicate consumes no slot");
            } else {
                budget_error(
                    builder.intern(node, None).expect_err("first extra"),
                    "closed generic data instances",
                    MAX_DATA_INSTANCES + 1,
                );
            }
        }
        assert_eq!(
            builder.types.iter().filter(|node| node.shape == TypeShape::Option).count(),
            MAX_DATA_INSTANCES
        );
    });
}

#[test]
fn ordered_edge_inventory_exact_duplicates_and_first_extra() {
    with_builder(|builder, _| {
        for from in 0..256u32 {
            let (from, _) = keys::encode(0x41, &[0, from], &[]).expect("synthetic root key");
            for to in 0..256u32 {
                let (to, _) =
                    keys::encode(0x14, &[1], &[&to.to_le_bytes()]).expect("synthetic target bytes");
                builder.edge(&from, &to, None).expect("exact edge");
                builder.edge(&from, &to, None).expect("duplicate edge");
            }
        }
        assert_eq!(builder.edges.len(), MAX_EDGES);
        assert!(builder.edges.windows(2).all(|pair| pair[0] < pair[1]));
        let (extra, _) = keys::encode(0x41, &[0, 256], &[]).expect("first extra root");
        budget_error(
            builder.edge(&extra, &[0x14], None).expect_err("first extra pair"),
            "instantiation edges",
            MAX_EDGES + 1,
        );
        assert_eq!(builder.edges.len(), MAX_EDGES);
    });
}

#[test]
fn key_storage_exact_first_extra_and_allocation_failure_are_distinct() {
    for (length, expected) in [(MAX_KEY_BYTES - 9, true), (MAX_KEY_BYTES - 8, false)] {
        let child = vec![0; length];
        let (key, size) = keys::encode(0x14, &[1], &[&child]).expect("bounded encoder");
        assert_eq!(!key.is_empty(), expected);
        assert_eq!(size, length + 9);
    }
    assert!(matches!(reserve::<u64>(usize::MAX), Err(InstantiationFailure::AllocationFailure)));
    assert_eq!(checked_count([usize::MAX, 0].into_iter()).expect("exact arithmetic"), usize::MAX);
    assert!(matches!(
        checked_count([usize::MAX, 1].into_iter()),
        Err(InstantiationFailure::InternalFailure)
    ));
}
