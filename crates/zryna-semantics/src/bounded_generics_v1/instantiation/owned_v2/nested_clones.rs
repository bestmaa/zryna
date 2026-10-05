//! Only two concrete depth-two clone forms; original source enters unchanged v3 authority.
use super::tests::{claim, with_claim};
use zryna_ir::generic_v1::owned_v2;

#[test]
fn nested_clone_seals_every_outer_and_inner_variant_with_original_owner_retained() {
    for (ty, construction) in [
        ("Option<Option<String>>", "Option.none<Option<String>>()"),
        ("Option<Option<String>>", "Option.some<Option<String>>(Option.none<String>())"),
        ("Option<Option<String>>", "Option.some<Option<String>>(Option.some<String>(\"入\"))"),
        (
            "Result<Option<String>,Option<String>>",
            "Result.ok<Option<String>,Option<String>>(Option.none<String>())",
        ),
        (
            "Result<Option<String>,Option<String>>",
            "Result.ok<Option<String>,Option<String>>(Option.some<String>(\"成功\"))",
        ),
        (
            "Result<Option<String>,Option<String>>",
            "Result.err<Option<String>,Option<String>>(Option.none<String>())",
        ),
        (
            "Result<Option<String>,Option<String>>",
            "Result.err<Option<String>,Option<String>>(Option.some<String>(\"失敗\"))",
        ),
    ] {
        let source = format!(
            "export function root(input:i32):i32 {{ const value:{ty} ={construction}; const view:Borrow<{ty}> =borrow(value); const first:{ty} =clone(value); const second:{ty} =clone(value); return input; }}"
        );
        with_claim(&[("main.zry", &source)], |claim, syntax, sources, linear, linux| {
            assert!(owned_v2::wire::encode(&claim).is_err());
            assert!(
                claim
                    .plans
                    .iter()
                    .flat_map(|p| &p.steps)
                    .any(|s| s.failure && s.end_loans.len() >= 4)
            );
            let runtime = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
                linear,
                linux,
            )
            .expect("runtime");
            owned_v2::verify(
                owned_v2::wire::v3::decode(&owned_v2::wire::v3::encode(&claim).expect("v3"))
                    .expect("decode"),
                syntax,
                sources,
                sources.verify_file_id(0).expect("entry"),
                linear,
                linux,
                &runtime,
            )
            .expect("independent nested source/CFG/loan/cleanup seal");
        });
    }
}

#[test]
fn nested_clone_whitelist_and_original_affinity_remain_exact() {
    for ty in [
        "Option<Result<String,String>>",
        "Result<Option<String>,String>",
        "Result<String,Option<String>>",
        "Option<Option<i32>>",
        "Option<Option<Option<String>>>",
    ] {
        let source = format!(
            "function unused(value:{ty}):i32 {{clone(value);return 0;}} export function root(input:i32):i32 {{return input;}}"
        );
        let zryna_ir::generic_v1::Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("exact finite whitelist")
        else {
            panic!("diagnostic")
        };
        assert_eq!(errors[0].code, "ZRYNA-M3008");
    }
}

#[test]
fn nested_execution_fixtures_seal_and_freeze_independent_source_dtos() {
    const VALUES: &str =
        include_str!("../../../../../../tests/m7-generic-owned-nested-clone/values.zry");
    const MAIN: &str =
        include_str!("../../../../../../tests/m7-generic-owned-nested-clone/main.zry");
    for cross in [false, true] {
        let joined = format!("{VALUES}\n{MAIN}");
        let imported = format!(
            "import {{option,result,makeOption,makeOk,makeErr,discardOption,discardResult}} from \"./values.zry\";\n{MAIN}"
        );
        let files = if cross {
            vec![("main.zry", imported.as_str()), ("values.zry", VALUES)]
        } else {
            vec![("main.zry", joined.as_str())]
        };
        if let Some(dir) = std::env::var_os("ZRYNA_GENERIC_OWNED_NESTED_CLONE_FIXTURE_DIR") {
            std::fs::write(
                std::path::PathBuf::from(dir).join(format!("{}-reference.json", files.len())),
                serde_json::to_vec_pretty(
                    &crate::bounded_generics_v1::tests::body_fixtures::snapshot(&files),
                )
                .expect("independent DTO"),
            )
            .expect("explicit new fixture output");
        }
        with_claim(&files, |claim, syntax, sources, linear, linux| {
            assert!(
                claim
                    .plans
                    .iter()
                    .flat_map(|p| &p.steps)
                    .any(|s| s.failure && s.end_loans.len() >= 5)
            );
            let runtime = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
                linear,
                linux,
            )
            .expect("runtime");
            owned_v2::verify(
                owned_v2::wire::v3::decode(&owned_v2::wire::v3::encode(&claim).expect("v3"))
                    .expect("decode"),
                syntax,
                sources,
                sources.verify_file_id(0).expect("entry"),
                linear,
                linux,
                &runtime,
            )
            .expect("frozen fixture source/CFG/loan/cleanup seal");
        });
    }
}
