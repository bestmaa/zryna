//! Structural clone stays concrete; v3 raw claims enter the same mandatory seal.
use super::tests::{claim, with_claim};
use std::fmt::Write;
use zryna_ir::generic_v1::owned_v2;

#[test]
fn concrete_option_and_result_clone_sources_seal_through_v3_only() {
    for (ty, construction) in [
        ("Option<String>", "Option.none<String>()"),
        ("Option<String>", "Option.some<String>(\"入\")"),
        ("Result<String,String>", "Result.ok<String,String>(\"ok\")"),
        ("Result<String,String>", "Result.err<String,String>(\"err\")"),
    ] {
        let source = format!(
            "export function root(input:i32):i32 {{ const value:{ty} = {construction}; const first:{ty} = clone(value); const second:{ty} = clone(value); return input; }}"
        );
        with_claim(&[("main.zry", &source)], |claim, syntax, sources, linear, linux| {
            assert!(
                claim.extensions.iter().flatten().any(|e| matches!(
                    e.operation,
                    owned_v2::raw::Operation::CloneBorrowedString(_)
                ))
            );
            assert!(owned_v2::wire::encode(&claim).is_err());
            let runtime = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
                linear,
                linux,
            )
            .expect("runtime");
            owned_v2::verify(
                owned_v2::wire::v3::decode(&owned_v2::wire::v3::encode(&claim).expect("v3 encode"))
                    .expect("v3 decode"),
                syntax,
                sources,
                sources.verify_file_id(0).expect("entry"),
                linear,
                linux,
                &runtime,
            )
            .expect("independent source/CFG/loan/cleanup seal");
        });
    }
}

#[test]
fn execution_fixtures_seal_and_freeze_independent_source_dtos() {
    const VALUES: &str = include_str!("../../../../../../tests/m7-generic-owned-clone/values.zry");
    const MAIN: &str = include_str!("../../../../../../tests/m7-generic-owned-clone/main.zry");
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
        if let Some(dir) = std::env::var_os("ZRYNA_GENERIC_OWNED_CLONE_FIXTURE_DIR") {
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
            assert!(owned_v2::wire::encode(&claim).is_err());
            let runtime = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
                linear,
                linux,
            )
            .expect("runtime");
            owned_v2::verify(
                owned_v2::wire::v3::decode(&owned_v2::wire::v3::encode(&claim).expect("v3 encode"))
                    .expect("decode"),
                syntax,
                sources,
                sources.verify_file_id(0).expect("entry"),
                linear,
                linux,
                &runtime,
            )
            .expect("complete clone source/CFG/loan/cleanup seal");
        });
    }
}

#[test]
fn clone_snapshot_credit_is_genuine_exact_first_extra_and_recovers() {
    let mut body = String::new();
    for i in 0..400 {
        write!(body, "const p{i}:i32=0;").expect("source fixture");
    }
    // Option<String> key has10bytes. Four complete snapshots per clone charge
    // 4962+36*j units:401bindings,410keybytes,next=401+9*j,outerloan/scope,
    // snapshots at next+1,+1,+2,+6;96precharged key/arm/edge units.
    for count in [140, 141, 1] {
        let source = format!(
            "function churn(value:Option<String>):i32 {{ {body} {} return 0; }} export function root(input:i32):i32 {{return input;}}",
            "clone(value);".repeat(count)
        );
        let result = claim(&[("main.zry", &source)]);
        if count == 141 {
            assert!(
                matches!(result,Err(zryna_ir::generic_v1::Failure::Diagnostics(v)) if v[0].code=="ZRYNA-M7201")
            );
        } else {
            result.expect("exact/recovery complete-state credit");
        }
    }
}
