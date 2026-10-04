//! Whole-local source replacement, exact originals, and independent cleanup evidence.
use super::tests::{claim, with_claim};
use zryna_ir::generic_v1::{Failure, owned_v2};
const VALUES: &str = include_str!("../../../../../../tests/m7-generic-owned-cfg/values.zry");
const MAIN: &str = include_str!("../../../../../../tests/m7-generic-owned-cfg/main.zry");

#[test]
fn whole_local_replacement_reinitialization_and_nested_returns_seal_in_both_module_forms() {
    for cross in [false, true] {
        let joined = format!("{VALUES}\n{MAIN}");
        let imported =
            format!("import {{discard,reinitialize,earlyIdentity}} from \"./values.zry\";\n{MAIN}");
        let files = if cross {
            vec![("main.zry", imported.as_str()), ("values.zry", VALUES)]
        } else {
            vec![("main.zry", joined.as_str())]
        };
        if let Some(dir) = std::env::var_os("ZRYNA_GENERIC_OWNED_CFG_FIXTURE_DIR") {
            std::fs::write(
                std::path::PathBuf::from(dir).join(format!("{}-reference.json", files.len())),
                serde_json::to_vec_pretty(
                    &crate::bounded_generics_v1::tests::body_fixtures::snapshot(&files),
                )
                .expect("DTO"),
            )
            .expect("explicit independent CFG fixture output");
        }
        with_claim(&files, |claim, syntax, sources, linear, linux| {
            let runtime = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
                linear,
                linux,
            )
            .expect("runtime");
            let verified = owned_v2::verify(
                owned_v2::wire::decode(&owned_v2::wire::encode(&claim).expect("wire"))
                    .expect("decode"),
                syntax,
                sources,
                sources.verify_file_id(0).expect("entry"),
                linear,
                linux,
                &runtime,
            )
            .expect("source/plan seal");
            assert_eq!(verified.scalar_abi().exports().len(), 7);
            // The replacement's failing allocation must still clean the original root.
            let (function, replacement) = claim.extensions.iter().enumerate().find_map(|(f, ext)| {
                let old = ext.iter().find(|e| matches!(&e.operation, owned_v2::raw::Operation::StringLiteral(b) if b == "α".as_bytes()))?;
                let new = ext.iter().find(|e| matches!(&e.operation, owned_v2::raw::Operation::StringLiteral(b) if b == b"new"))?;
                Some((f, (old.result, new.result)))
            }).expect("replacement witness");
            let block = &claim.graph.functions[function].blocks[0];
            let position = block
                .instructions
                .iter()
                .position(|i| i.result.id == replacement.1)
                .expect("fallible instruction");
            let failure = claim.plans[function]
                .steps
                .iter()
                .find(|s| s.failure && s.block == 0 && s.position as usize == position)
                .expect("exact failure step");
            assert!(failure.cleanup.contains(&replacement.0));
            assert!(!failure.cleanup.contains(&replacement.1));
            // Raw cleanup claims cannot omit the still-live old value.
            let mut hostile = claim.clone();
            hostile.plans[function]
                .steps
                .iter_mut()
                .find(|s| s.failure && s.block == 0 && s.position as usize == position)
                .expect("failure")
                .cleanup
                .retain(|id| *id != replacement.0);
            assert!(
                owned_v2::verify(
                    owned_v2::wire::decode(
                        &owned_v2::wire::encode(&hostile).expect("hostile wire")
                    )
                    .expect("decode"),
                    syntax,
                    sources,
                    sources.verify_file_id(0).expect("entry"),
                    linear,
                    linux,
                    &runtime
                )
                .is_err()
            );
        });
    }
}

#[test]
fn original_mutable_affinity_and_loan_conflicts_fail_at_the_exact_target_or_read() {
    let bodies = [
        (
            "let item:String=\"α\"; const view:Borrow<String> =borrow(item); item=\"new\";",
            "item=\"new\"",
        ),
        ("let item:i32=input; const view:Borrow<i32> =borrow(item); item=8;", "item=8"),
        ("let item:String=\"α\"; const moved:String=item; const twice:String=item;", "item; }"),
        (
            "let item:i32=input; const view:BorrowMut<i32> =borrowMut(item); const read:i32=item;",
            "item; }",
        ),
    ];
    for (body, witness) in bodies {
        let source = format!(
            "function unused(input:i32):i32 {{ {body} }} export function root(input:i32):i32 {{ return input; }}"
        );
        // Add the required return after the hostile operation while keeping the token witness unique.
        let source = source.replacen("; }", "; return input; }", 1);
        let failure =
            claim(&[("main.zry", &source)]).expect_err("unused original rejected before layout");
        let Failure::Diagnostics(errors) = failure else { panic!("source diagnostics") };
        assert_eq!(errors[0].code, "ZRYNA-M7007");
        let span = errors[0].primary_span().expect("exact source target/read");
        assert_eq!(&source[span.start() as usize..span.end() as usize], "item");
        let expected =
            source.rfind(witness.split(';').next().expect("witness")).expect("last hostile token");
        assert_eq!(span.start() as usize, expected);
    }
}

#[test]
fn opaque_mutable_reinitialization_never_legalizes_a_second_original_move() {
    let source = "function bad<T extends ZrynaValue>(value:T,next:T):T { let item:T=value; const moved:T=item; item=next; const once:T=item; return item; } export function root(input:i32):i32 { return bad<i32>(input,input); }";
    let Failure::Diagnostics(errors) =
        claim(&[("main.zry", source)]).expect_err("opaque affinity survives Copy substitution")
    else {
        panic!("source diagnostic")
    };
    assert_eq!(errors[0].code, "ZRYNA-M7007");
    let span = errors[0].primary_span().expect("exact repeated read");
    assert_eq!(span.start() as usize, source.find("return item").expect("witness") + 7);
}

#[test]
fn conditional_and_loop_state_still_require_a_separate_successor_proof() {
    for body in [
        "if(true) { return input; } else { return input; }",
        "while(false) { const item:String=\"unused\"; } return input;",
    ] {
        let source = format!(
            "function unused(input:i32):i32 {{ {body} }} export function root(input:i32):i32 {{ return input; }}"
        );
        let Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("not admitted by whole-local proof")
        else {
            panic!("source diagnostic")
        };
        assert_eq!(errors[0].code, "ZRYNA-M3008");
    }
}
