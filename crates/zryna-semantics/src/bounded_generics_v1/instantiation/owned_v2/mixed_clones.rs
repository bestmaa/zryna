//! Exact asymmetric Result forms, fixed six paths and independent source snapshot credits.
use super::tests::{claim, with_claim};
use std::fmt::Write;
use zryna_ir::generic_v1::{Failure, owned_v2};

#[test]
fn asymmetric_result_seals_six_variants_with_repeated_clones_and_retained_source() {
    for (ty, constructor) in [
        (
            "Result<Option<String>,String>",
            "Result.ok<Option<String>,String>(Option.none<String>())",
        ),
        (
            "Result<Option<String>,String>",
            "Result.ok<Option<String>,String>(Option.some<String>(\"成功\"))",
        ),
        ("Result<Option<String>,String>", "Result.err<Option<String>,String>(\"失敗\")"),
        ("Result<String,Option<String>>", "Result.ok<String,Option<String>>(\"直接\")"),
        (
            "Result<String,Option<String>>",
            "Result.err<String,Option<String>>(Option.none<String>())",
        ),
        (
            "Result<String,Option<String>>",
            "Result.err<String,Option<String>>(Option.some<String>(\"間接\"))",
        ),
    ] {
        let source = format!(
            "function keep(value:{ty}):{ty} {{{{const view:Borrow<{ty}> =borrow(value);const first:{ty} =clone(value);const second:{ty} =clone(value);}}return value;}} export function root(input:i32):i32 {{const value:{ty} ={constructor};const result:{ty} =keep(value);return input;}}"
        );
        with_claim(&[("main.zry", &source)], |c, syntax, sources, linear, linux| {
            assert!(owned_v2::wire::encode(&c).is_err());
            let abi = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
                linear,
                linux,
            )
            .expect("runtime");
            owned_v2::verify(
                owned_v2::wire::v3::decode(&owned_v2::wire::v3::encode(&c).expect("v3"))
                    .expect("decode"),
                syntax,
                sources,
                sources.verify_file_id(0).expect("entry"),
                linear,
                linux,
                &abi,
            )
            .expect("exact asymmetric source and loan seal");
        });
    }
}

#[test]
fn asymmetric_result_fixed_execution_fixtures_seal_both_module_forms() {
    const VALUES: &str =
        include_str!("../../../../../../tests/m7-generic-owned-mixed-clone/values.zry");
    const MAIN: &str =
        include_str!("../../../../../../tests/m7-generic-owned-mixed-clone/main.zry");
    for cross in [false, true] {
        let joined = format!("{VALUES}\n{MAIN}");
        let imported = format!(
            "import {{left,right,makeLeftSome,makeLeftString,makeRightString,makeRightSome,discardLeft,discardRight}} from \"./values.zry\";\n{MAIN}"
        );
        let files = if cross {
            vec![("main.zry", imported.as_str()), ("values.zry", VALUES)]
        } else {
            vec![("main.zry", joined.as_str())]
        };
        if let Some(dir) = std::env::var_os("ZRYNA_GENERIC_OWNED_MIXED_CLONE_FIXTURE_DIR") {
            std::fs::write(
                std::path::PathBuf::from(dir).join(format!("{}-reference.json", files.len())),
                serde_json::to_vec_pretty(
                    &crate::bounded_generics_v1::tests::body_fixtures::snapshot(&files),
                )
                .expect("independent DTO"),
            )
            .expect("explicit fixture output");
        }
        with_claim(&files, |c, syntax, sources, linear, linux| {
            let abi = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
                linear,
                linux,
            )
            .expect("runtime");
            assert!(
                c.plans.iter().flat_map(|p| &p.steps).any(|s| s.failure && s.end_loans.len() >= 5)
            );
            assert!(
                c.plans.iter().flat_map(|p| &p.steps).any(|s| s.failure && s.end_loans.len() == 4)
            );
            owned_v2::verify(
                owned_v2::wire::v3::decode(&owned_v2::wire::v3::encode(&c).expect("v3"))
                    .expect("decode"),
                syntax,
                sources,
                sources.verify_file_id(0).expect("entry"),
                linear,
                linux,
                &abi,
            )
            .expect("source/inventory/typed CFG/loan/cleanup seal");
        });
    }
}

#[test]
fn asymmetric_result_snapshot_credits_exact_first_extra_and_recovery() {
    let mut body = String::new();
    for i in 0..400 {
        write!(body, "const p{i}:i32=0;").expect("fixture");
    }
    // 401 bindings, 424 key bytes. Eight snapshots and seventeen definitions/clone.
    // Root key:24*8+16=208; one Option child:20 key copies +96 selection.
    // Option-first snapshots:1229,1229,1232,1232,1233,1237,1238,1242.
    // String-first snapshots:1229,1229,1233,1236,1236,1237,1241,1242.
    // Credits:10196+136*j /10207+136*j. At n70:1042160/1042930;
    // n71:1061876/1062657. The unchanged ceiling is1048576.
    for ty in ["Result<Option<String>,String>", "Result<String,Option<String>>"] {
        for count in [70, 71, 1] {
            let source = format!(
                "function churn(value:{ty}):i32 {{{body} {} return 0;}} export function root(input:i32):i32 {{return input;}}",
                "clone(value);".repeat(count)
            );
            let result = claim(&[("main.zry", &source)]);
            if count == 71 {
                assert!(
                    matches!(result,Err(Failure::Diagnostics(v)) if v[0].code=="ZRYNA-M7201"),
                    "{ty} first extra"
                );
            } else {
                result.expect("independent exact credits and pristine recovery");
            }
        }
    }
}
