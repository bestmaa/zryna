//! Hostile successor Copy authority tests.

mod fixtures;
mod opaque_owners;
mod wire;
use super::*;
use crate::generic_v1::wire::{decode, encode};
use fixtures::{Fixture, authorities, key};
use zryna_ownership_runtime_abi::generic_v1 as runtime;

fn check(fixture: &Fixture, program: &raw::Program) -> Result<(), Failure> {
    let abi = runtime::verify_v1(
        runtime::raw_v1(&fixture.linear, &fixture.linux).expect("genuine fixture invariant"),
        &fixture.linear,
        &fixture.linux,
    )
    .expect("genuine fixture invariant");
    verify(
        decode(&encode(program)?)?,
        &fixture.syntax,
        &fixture.sources,
        fixture.sources.verify_file_id(0).expect("genuine fixture invariant"),
        &fixture.linear,
        &fixture.linux,
        &abi,
    )
    .map(|_| ())
}

#[test]
fn complete_source_body_demand_wire_and_empty_effects_seal_copy_program() {
    let fixture = Fixture::new();
    assert_eq!(fixture.raw.functions.len(), 11);
    check(&fixture, &fixture.raw).expect("genuine fixture invariant");
    let abi = runtime::verify_v1(
        runtime::raw_v1(&fixture.linear, &fixture.linux).expect("genuine fixture invariant"),
        &fixture.linear,
        &fixture.linux,
    )
    .expect("genuine fixture invariant");
    let sealed = verify(
        decode(&encode(&fixture.raw).expect("genuine fixture invariant"))
            .expect("genuine fixture invariant"),
        &fixture.syntax,
        &fixture.sources,
        fixture.sources.verify_file_id(0).expect("genuine fixture invariant"),
        &fixture.linear,
        &fixture.linux,
        &abi,
    )
    .expect("genuine fixture invariant");
    assert_eq!(sealed.ownership_effects(), (0, 0));
    assert_eq!(sealed.scalar_abi().exports().len(), 6);
}

#[test]
fn typed_but_fabricated_literal_return_and_order_cannot_replace_source() {
    let fixture = Fixture::new();
    for change in 0..3 {
        let mut program = fixture.raw.clone();
        let root = program
            .functions
            .iter_mut()
            .find(|function| function.public_export.as_deref() == Some("score"))
            .expect("score root");
        match change {
            0 => {
                // Arbitrary well-typed root body omits construction, call and match.
                root.blocks = vec![raw::Block {
                    id: 0,
                    parameters: vec![raw::Definition { id: 0, ty: raw::Type::Stored(1) }],
                    instructions: vec![],
                    span: root.span,
                    terminator: raw::Terminator::Return(0),
                }];
            }
            1 => {
                root.blocks.last_mut().expect("genuine fixture invariant").terminator =
                    raw::Terminator::Return(0);
            }
            _ => {
                let instruction = program
                    .functions
                    .iter_mut()
                    .flat_map(|function| &mut function.blocks)
                    .flat_map(|block| &mut block.instructions)
                    .find(|instruction| {
                        matches!(instruction.operation, raw::Operation::I32Literal(11))
                    })
                    .expect("genuine fixture invariant");
                instruction.operation = raw::Operation::I32Literal(12);
            }
        }
        crate::generic_v1::validate_source_graph(
            &program,
            &fixture.syntax,
            &fixture.sources,
            &fixture.linear,
            &fixture.linux,
        )
        .expect("genuine fixture invariant");
        assert!(check(&fixture, &program).is_err());
    }
    check(&fixture, &fixture.raw).expect("genuine fixture invariant");
}

#[test]
fn missing_and_extra_generic_instances_do_not_enter_executable_demand() {
    let fixture = Fixture::new();
    let mut extra = fixture
        .raw
        .functions
        .iter()
        .filter(|function| function.key[0] == 0x40)
        .map(|function| function.key.clone())
        .collect::<Vec<_>>();
    extra.push(key(1, 0));
    assert!(
        produce_claim(
            &fixture.syntax,
            &fixture.sources,
            &fixture.linear,
            &fixture.linux,
            &extra.iter().map(Vec::as_slice).collect::<Vec<_>>()
        )
        .is_err()
    );
    let missing = fixture
        .raw
        .functions
        .iter()
        .filter(|function| function.key[0] == 0x40 && function.key != key(0, 0))
        .map(|function| function.key.clone())
        .collect::<Vec<_>>();
    assert!(
        produce_claim(
            &fixture.syntax,
            &fixture.sources,
            &fixture.linear,
            &fixture.linux,
            &missing.iter().map(Vec::as_slice).collect::<Vec<_>>()
        )
        .is_err()
    );
}

#[test]
fn lexical_value_shadowing_original_function_cannot_issue_a_closed_call() {
    let (sources, syntax, linear, linux) = authorities(
        include_str!("../../../../../tests/m7-generic-copy-fixtures/shadow.zry"),
        include_bytes!("../../../../../tests/m7-generic-copy-fixtures/shadow.json"),
        false,
    );
    assert!(produce_claim(&syntax, &sources, &linear, &linux, &[&key(0, 1)]).is_err());
}

#[test]
fn unused_opaque_template_is_checked_before_any_closed_specialization() {
    let (sources, syntax, linear, linux) = authorities(
        include_str!("../../../../../tests/m7-generic-copy-fixtures/opaque.zry"),
        include_bytes!("../../../../../tests/m7-generic-copy-fixtures/opaque.json"),
        false,
    );
    assert!(produce_claim(&syntax, &sources, &linear, &linux, &[]).is_err());
}

#[test]
fn structurally_valid_owned_root_is_rejected_without_a_cleanup_issuer() {
    let (sources, syntax, linear, linux) = authorities(
        include_str!("../../../../../tests/m7-generic-copy-fixtures/owned.zry"),
        include_bytes!("../../../../../tests/m7-generic-copy-fixtures/owned.json"),
        false,
    );
    let raw =
        produce_claim(&syntax, &sources, &linear, &linux, &[]).expect("genuine fixture invariant");
    crate::generic_v1::validate_source_graph(&raw, &syntax, &sources, &linear, &linux)
        .expect("genuine fixture invariant");
    let abi = runtime::verify_v1(
        runtime::raw_v1(&linear, &linux).expect("genuine fixture invariant"),
        &linear,
        &linux,
    )
    .expect("genuine fixture invariant");
    assert!(
        verify(
            decode(&encode(&raw).expect("genuine fixture invariant"))
                .expect("genuine fixture invariant"),
            &syntax,
            &sources,
            sources.verify_file_id(0).expect("genuine fixture invariant"),
            &linear,
            &linux,
            &abi
        )
        .is_err()
    );
}

#[test]
fn equal_text_foreign_source_runtime_or_entry_cannot_replay() {
    let first = Fixture::new();
    let second = Fixture::new();
    let abi = runtime::verify_v1(
        runtime::raw_v1(&second.linear, &second.linux).expect("genuine fixture invariant"),
        &second.linear,
        &second.linux,
    )
    .expect("genuine fixture invariant");
    assert!(
        verify(
            decode(&encode(&first.raw).expect("genuine fixture invariant"))
                .expect("genuine fixture invariant"),
            &first.syntax,
            &first.sources,
            first.sources.verify_file_id(0).expect("genuine fixture invariant"),
            &first.linear,
            &first.linux,
            &abi
        )
        .is_err()
    );
    let abi = runtime::verify_v1(
        runtime::raw_v1(&first.linear, &first.linux).expect("genuine fixture invariant"),
        &first.linear,
        &first.linux,
    )
    .expect("genuine fixture invariant");
    assert!(
        verify(
            decode(&encode(&first.raw).expect("genuine fixture invariant"))
                .expect("genuine fixture invariant"),
            &first.syntax,
            &first.sources,
            second.sources.verify_file_id(0).expect("genuine fixture invariant"),
            &first.linear,
            &first.linux,
            &abi
        )
        .is_err()
    );
}

#[test]
fn foreign_verified_function_identity_cannot_select_a_local_instance() {
    let first = Fixture::new();
    let second = Fixture::new();
    let a = runtime::verify_v1(
        runtime::raw_v1(&first.linear, &first.linux).expect("first contract"),
        &first.linear,
        &first.linux,
    )
    .expect("first runtime authority");
    let b = runtime::verify_v1(
        runtime::raw_v1(&second.linear, &second.linux).expect("second contract"),
        &second.linear,
        &second.linux,
    )
    .expect("second runtime authority");
    let left = verify(
        decode(&encode(&first.raw).expect("first wire")).expect("first decode"),
        &first.syntax,
        &first.sources,
        first.sources.verify_file_id(0).expect("first entry"),
        &first.linear,
        &first.linux,
        &a,
    )
    .expect("first program");
    let right = verify(
        decode(&encode(&second.raw).expect("second wire")).expect("second decode"),
        &second.syntax,
        &second.sources,
        second.sources.verify_file_id(0).expect("second entry"),
        &second.linear,
        &second.linux,
        &b,
    )
    .expect("second program");
    let id = left.function_id(0).expect("first generic identity");
    assert!(left.function(id).is_some());
    assert!(right.function(id).is_none());
}

#[test]
fn physically_valid_extra_stored_type_cannot_enter_exact_source_demand() {
    use zryna_layout::{
        generic_v1::{raw as types, verify},
        raw::TypeKind as Base,
    };
    let mut fixture = Fixture::new();
    let kinds = vec![
        types::TypeKind::Base(Base::Bool),
        types::TypeKind::Base(Base::I32),
        types::TypeKind::Base(Base::String),
        types::TypeKind::Option { argument: types::NodeId(1) },
        types::TypeKind::Result { okay: types::NodeId(0), error: types::NodeId(1) },
        types::TypeKind::Base(Base::Vec { element: types::NodeId(1) }),
    ];
    let graph = types::Graph {
        modules: vec![types::Module {
            id: types::ModuleId(0),
            source_file: fixture.sources.verify_file_id(0).expect("entry"),
            data_declarations: 0,
        }],
        declarations: vec![],
        types: kinds
            .into_iter()
            .enumerate()
            .map(|(index, kind)| types::TypeNode {
                id: types::NodeId(u32::try_from(index).expect("index")),
                span: None,
                kind,
            })
            .collect(),
        program_roots: (0..6).map(types::NodeId).collect(),
    };
    fixture.linear = verify(&graph, &fixture.sources, StorageTarget::Linear32V1)
        .expect("independent valid linear graph");
    fixture.linux = verify(&graph, &fixture.sources, StorageTarget::LinuxX8664V1)
        .expect("independent valid Linux graph");
    fixture.raw.universe = *fixture.linear.universe_identity();
    fixture.raw.linear32 = *fixture.linear.fingerprint();
    fixture.raw.linux_x86_64 = *fixture.linux.fingerprint();
    fixture.raw.type_keys = fixture.linear.types().map(|ty| ty.key().to_vec()).collect();
    crate::generic_v1::validate_source_graph(
        &fixture.raw,
        &fixture.syntax,
        &fixture.sources,
        &fixture.linear,
        &fixture.linux,
    )
    .expect("old partial source check remains insufficient for exact type demand");
    assert!(check(&fixture, &fixture.raw).is_err());
}
