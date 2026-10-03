//! Independently authored, well-typed Copy claims must not legalize an invalid original owner.

use super::{Fixture, authorities, check, key, raw};
use zryna_syntax::v5::RawExpressionKind;

#[test]
fn copy_instance_cannot_duplicate_the_original_opaque_owner() {
    reject_duplicate(true);
}

#[test]
fn unused_original_cannot_hide_opaque_owner_duplication() {
    reject_duplicate(false);
}

fn reject_duplicate(used: bool) {
    let fixture = fixture(used);
    crate::generic_v1::validate_source_graph(
        &fixture.raw,
        &fixture.syntax,
        &fixture.sources,
        &fixture.linear,
        &fixture.linux,
    )
    .expect("fixed closed i32 claims pass the partial source/type graph");
    let super::Failure::Diagnostics(errors) =
        check(&fixture, &fixture.raw).expect_err("original owner duplication")
    else {
        panic!("fixed source ownership diagnostic");
    };
    assert!(errors.iter().any(|error| {
        error.code == "ZRYNA-I7001"
            && error.message.contains("Copy specialization cannot duplicate")
    }));
    let generic = fixture
        .raw
        .functions
        .iter()
        .filter(|f| f.key[0] == 0x40)
        .map(|f| f.key.as_slice())
        .collect::<Vec<_>>();
    assert!(
        super::produce_claim(
            &fixture.syntax,
            &fixture.sources,
            &fixture.linear,
            &fixture.linux,
            &generic,
        )
        .is_err()
    );
}

fn fixture(used: bool) -> Fixture {
    let (source, snapshot): (&str, &[u8]) = if used {
        (
            include_str!("../../../../../../tests/m7-generic-copy-fixtures/duplicate-owner.zry"),
            include_bytes!("../../../../../../tests/m7-generic-copy-fixtures/duplicate-owner.json"),
        )
    } else {
        (
            include_str!(
                "../../../../../../tests/m7-generic-copy-fixtures/unused-duplicate-owner.zry"
            ),
            include_bytes!(
                "../../../../../../tests/m7-generic-copy-fixtures/unused-duplicate-owner.json"
            ),
        )
    };
    let (sources, syntax, linear, linux) = authorities(source, snapshot, false);
    let unit = &syntax.files()[0];
    let declarations = unit
        .functions
        .iter()
        .enumerate()
        .map(|(index, f)| raw::Declaration {
            module: 0,
            function: u32::try_from(index).expect("three fixed original functions"),
            parameters: u32::from(index != 2),
            span: f.span,
        })
        .collect();
    let mut functions = Vec::new();
    for index in if used { vec![0usize, 1, 2] } else { vec![2] } {
        let original = &unit.functions[index];
        let call = original
            .body
            .expressions
            .iter()
            .find(|e| matches!(e.kind, RawExpressionKind::Call { .. }));
        let instructions = call.map_or_else(Vec::new, |call| {
            vec![raw::Instruction {
                result: raw::Definition { id: 1, ty: raw::Type::Stored(1) },
                span: call.span,
                operation: raw::Operation::ClosedGenericCall {
                    instance: u32::from(index != 1),
                    arguments: vec![0],
                },
            }]
        });
        let mut function_key = if index == 2 {
            vec![0x41]
        } else {
            key(u32::try_from(index).expect("three fixed original functions"), 1)
        };
        if index == 2 {
            function_key.extend_from_slice(&0u32.to_le_bytes());
            function_key.extend_from_slice(&2u32.to_le_bytes());
        }
        functions.push(raw::Function {
            key: function_key,
            span: original.span,
            public_export: (index == 2).then(|| "run".into()),
            parameters: vec![raw::Type::Stored(1)],
            result: raw::Type::Stored(1),
            blocks: vec![raw::Block {
                id: 0,
                parameters: vec![raw::Definition { id: 0, ty: raw::Type::Stored(1) }],
                instructions,
                span: original.body.statements.last().expect("fixed return").span,
                terminator: raw::Terminator::Return(u32::from(index == 2 && used)),
            }],
        });
    }
    let program = raw::Program {
        modules: vec![raw::Module { id: 0, functions: 3 }],
        declarations,
        type_keys: vec![vec![0], vec![1], vec![2]],
        universe: *linear.universe_identity(),
        linear32: *linear.fingerprint(),
        linux_x86_64: *linux.fingerprint(),
        functions,
    };
    Fixture { sources, syntax, linear, linux, raw: program }
}
