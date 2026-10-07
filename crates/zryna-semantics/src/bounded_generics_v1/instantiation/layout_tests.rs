//! Authenticated v5 source-to-closed-layout checks, separate from raw-layout hostile tests.

use std::fmt::Write as _;

use super::{InstanceContext, discover, layouts::verify_layouts};
use crate::bounded_generics_v1::tests::body_fixtures::project;
use crate::bounded_generics_v1::{SemanticInput, body_types, resolve_declarations};
use zryna_layout::StorageTarget;
use zryna_source::NormalizedSourcePath;

fn checked(files: &[(&str, &str)], test: impl FnOnce(&InstanceContext<'_, '_, '_>)) {
    let input = project(files);
    let entry = input
        .sources
        .file_id(&NormalizedSourcePath::new("main.zry").expect("path"))
        .expect("entry");
    let declarations = resolve_declarations(
        SemanticInput::try_new(&input.syntax, &input.sources, entry)
            .expect("exact v5 source authority"),
    )
    .expect("original declarations");
    let bodies = body_types::check_body_types(&declarations).expect("all original opaque bodies");
    let instances = discover(&bodies).expect("bounded complete closed inventory");
    test(&instances);
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::new();
    for byte in bytes {
        write!(text, "{byte:02x}").expect("fixture hex");
    }
    text
}

#[test]
fn authenticated_box_source_reproduces_fixed_successor_layout_on_both_targets() {
    checked(
        &[(
            "main.zry",
            "interface Box<T extends ZrynaValue> extends ZrynaStruct {value:T;} function read(x:Box<i32>):i32 {return 0;}",
        )],
        |instances| {
            for (target, digest) in [
                (
                    StorageTarget::Linear32V1,
                    "bd03f54af9c19bd015ccb804fa1506043e9ed427810f1445ca0a2e24506ab3c7",
                ),
                (
                    StorageTarget::LinuxX8664V1,
                    "76f709b0de15bf2adc3974e30551052cfd069b0673afc04e0e89a4016b7728bb",
                ),
            ] {
                let layouts =
                    verify_layouts(instances, target).expect("independent closed layout seal");
                assert_eq!(
                    layouts.source_map_identity(),
                    instances.bodies().declarations().sources().identity()
                );
                assert_eq!(hex(layouts.fingerprint()), digest);
                let boxed = layouts.types().find(|ty| ty.key()[0] == 0x12).expect("closed Box");
                assert_eq!((boxed.size(), boxed.alignment()), (4, 4));
                assert_eq!(
                    boxed
                        .arguments()
                        .map(zryna_layout::generic_v1::TypeId::index)
                        .collect::<Vec<_>>(),
                    [1]
                );
                assert_eq!(
                    boxed
                        .fields()
                        .map(|(ordinal, ty, offset)| (ordinal, ty.index(), offset))
                        .collect::<Vec<_>>(),
                    [(0, 1, 0)]
                );
            }
        },
    );
}

#[test]
fn authenticated_option_and_result_sources_reproduce_fixed_dual_target_fingerprints() {
    for (source, linear, linux) in [
        (
            "function read(x:Option<i32>):i32 {return 0;}",
            "1701d9b3c46f81f99b2e1e992528a08dde5956364de423526ddfb8b9e796925e",
            "8d43a2e14c32cfe7c879fbadce3447889bf32107b0ccd38a6e5e193c8928c9b0",
        ),
        (
            "function read(x:Result<i32,bool>):i32 {return 0;}",
            "5dd551e3f249eb561826191ad9b72888ea839ea4d2f86c269f255039cfd545cc",
            "995c0a8b8501959c2b5d574c841c8dc33833a44374def43cd2d421b555003880",
        ),
    ] {
        checked(&[("main.zry", source)], |instances| {
            for (target, digest) in
                [(StorageTarget::Linear32V1, linear), (StorageTarget::LinuxX8664V1, linux)]
            {
                assert_eq!(
                    hex(verify_layouts(instances, target)
                        .expect("compiler-owned family seal")
                        .fingerprint()),
                    digest
                );
            }
        });
    }
}

#[test]
fn authenticated_owned_standard_enum_source_seals_the_fixed_nested_universe_prefix() {
    checked(
        &[(
            "main.zry",
            "function nested(x:Option<Option<String>>):i32 {return 0;} function result(x:Result<Option<String>,String>):i32 {return 0;}",
        )],
        |instances| {
            let fixtures: serde_json::Value = serde_json::from_str(include_str!(
                "../../../../../spec/memory-model/generic-owned-layout-v1-fixtures.json"
            ))
            .expect("independent owned references");
            let expected = &fixtures["cases"][2];
            for target in [StorageTarget::Linear32V1, StorageTarget::LinuxX8664V1] {
                let layouts =
                    verify_layouts(instances, target).expect("owned closed source prefix");
                assert_eq!(layouts.types().len(), 6);
                let expected =
                    &expected["targets"][usize::from(target == StorageTarget::LinuxX8664V1)];
                assert_eq!(
                    hex(layouts.fingerprint()),
                    expected["sha256"].as_str().expect("fixed whole prefix digest")
                );
                let result = layouts.types().find(|ty| ty.key()[0] == 0x15).expect("owned Result");
                assert_eq!(result.drop_kind(), 1);
                assert_eq!(result.runtime_kind(), 1);
            }
        },
    );
}

#[test]
fn authenticated_imported_aliases_preserve_original_nominal_identity_and_distinct_arguments() {
    checked(
        &[
            (
                "main.zry",
                "import { Box as Crate } from \"./values.zry\"; function number(x:Crate<i32>):i32 {return 0;} function flag(x:Crate<bool>):bool {return true;}",
            ),
            (
                "values.zry",
                "export interface Box<T extends ZrynaValue> extends ZrynaStruct {value:T;}",
            ),
        ],
        |instances| {
            let layouts = verify_layouts(instances, StorageTarget::Linear32V1)
                .expect("imported closed instances");
            let boxes = layouts.types().filter(|ty| ty.key()[0] == 0x12).collect::<Vec<_>>();
            assert_eq!(boxes.len(), 2);
            for boxed in &boxes {
                assert_eq!(&boxed.key()[1..5], &1u32.to_le_bytes());
            }
            assert_ne!(boxes[0].id(), boxes[1].id());
            assert_eq!(
                boxes
                    .iter()
                    .map(|boxed| boxed.arguments().next().expect("argument").index())
                    .collect::<Vec<_>>(),
                [0, 1]
            );
        },
    );
}

#[test]
fn authenticated_generic_enum_keeps_unit_variant_and_substituted_payload_ordinals() {
    checked(
        &[(
            "main.zry",
            "interface Choice<T extends ZrynaValue,E extends ZrynaValue> extends ZrynaEnum {none:ZrynaNone;left:T;right:E;} function read(x:Choice<i32,bool>):i32 {return 0;}",
        )],
        |instances| {
            let layouts =
                verify_layouts(instances, StorageTarget::LinuxX8664V1).expect("closed source enum");
            let choice = layouts.types().find(|ty| ty.key()[0] == 0x13).expect("source enum key");
            assert_eq!(
                choice
                    .variants()
                    .map(|(ordinal, ty)| (ordinal, ty.map(zryna_layout::generic_v1::TypeId::index)))
                    .collect::<Vec<_>>(),
                [(0, None), (1, Some(1)), (2, Some(0))]
            );
        },
    );
}

#[test]
fn authenticated_by_value_cycle_rejects_and_vec_indirection_is_layout_finite() {
    for (member, accepted) in [("Node<T>", false), ("Vec<Node<T>>", true)] {
        let source = format!(
            "interface Node<T extends ZrynaValue> extends ZrynaStruct {{next:{member};}} function read(x:Node<i32>):i32 {{return 0;}}"
        );
        checked(&[("main.zry", &source)], |instances| {
            let result = verify_layouts(instances, StorageTarget::Linear32V1);
            if accepted {
                assert!(result.is_ok());
            } else {
                let Err(zryna_layout::generic_v1::Failure::Diagnostics(errors)) = result else {
                    panic!("by-value layout rejection");
                };
                assert!(errors.iter().any(|error| error.code() == "ZRYNA-L3002"));
            }
        });
    }
}

#[test]
fn authenticated_unused_nongeneric_borrow_member_cannot_receive_a_stored_layout() {
    checked(
        &[(
            "main.zry",
            "interface Bad extends ZrynaStruct {loan:Borrow<i32>;} function score():i32 {return 7;}",
        )],
        |instances| {
            let Err(zryna_layout::generic_v1::Failure::Diagnostics(errors)) =
                verify_layouts(instances, StorageTarget::Linear32V1)
            else {
                panic!("stored borrow rejection");
            };
            assert_eq!(errors[0].code(), "ZRYNA-L3004");
            assert!(errors[0].primary_span().is_some());
        },
    );
}

fn result_tree(leaves: usize) -> String {
    if leaves == 1 {
        return "i32".into();
    }
    format!("Result<{},{}>", result_tree(leaves / 2), result_tree(leaves - leaves / 2))
}

#[test]
fn authenticated_complete_4096_byte_key_seals_and_4097_rejects_before_layout() {
    let argument = result_tree(291);
    let exact = format!(
        "interface Box<T extends ZrynaValue> extends ZrynaStruct {{value:T;}} function read(x:Option<Option<Box<{argument}>>>):i32 {{return 0;}}"
    );
    checked(&[("main.zry", &exact)], |instances| {
        assert!(instances.type_keys().any(|key| key.len() == 4096));
        for target in [StorageTarget::Linear32V1, StorageTarget::LinuxX8664V1] {
            assert!(
                verify_layouts(instances, target)
                    .expect("exact key complete layout")
                    .types()
                    .any(|ty| ty.key().len() == 4096)
            );
        }
    });
    let extra =
        format!("function read(x:Option<Option<Option<Option<{argument}>>>>):i32 {{return 0;}}");
    let super::InstantiationFailure::Diagnostics(errors) =
        super::tests::check(&[("main.zry", &extra)]).expect_err("complete first-extra key")
    else {
        panic!("source key rejection");
    };
    assert_eq!(errors[0].code(), "ZRYNA-M7201");
    assert!(errors[0].message().contains("rejected count 4097"));
    assert!(errors[0].primary_span().is_some());
}
