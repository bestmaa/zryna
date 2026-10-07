//! Original ownership and byte authentication cannot be bypassed by asymmetric payload types.
use super::tests::claim;
use zryna_ir::generic_v1::Failure;

#[test]
fn asymmetric_result_rejects_moves_exclusive_loans_temporaries_and_nearby_types() {
    for ty in ["Result<Option<String>,String>", "Result<String,Option<String>>"] {
        for prefix in [
            format!("const moved:{ty} =value;"),
            format!("const view:BorrowMut<{ty}> =borrowMut(value);"),
        ] {
            let s = format!(
                "function unused(value:{ty}):i32 {{{prefix} clone(value);return 7;}} export function root(input:i32):i32 {{return input;}}"
            );
            let Failure::Diagnostics(v) =
                claim(&[("main.zry", &s)]).expect_err("original owner exclusion")
            else {
                panic!("diagnostic")
            };
            assert_eq!(v[0].code, "ZRYNA-M7007");
            let span = v[0].primary_span().expect("owner witness");
            assert!(s[span.start() as usize..span.end() as usize].contains("value"));
        }
    }
    for expression in [
        "Result.ok<Option<String>,String>(Option.none<String>())",
        "Result.err<String,Option<String>>(Option.none<String>())",
    ] {
        let s =
            format!("export function root(input:i32):i32 {{clone({expression});return input;}}");
        assert!(
            matches!(claim(&[("main.zry",&s)]),Err(Failure::Diagnostics(v)) if v[0].code=="ZRYNA-M7007")
        );
    }
    for ty in [
        "Result<Option<String>,bool>",
        "Result<bool,Option<String>>",
        "Result<Option<Option<String>>,String>",
        "Result<String,Option<Option<String>>>",
        "Result<Option<i32>,String>",
        "Result<String,Option<i32>>",
        "Option<Result<String,String>>",
    ] {
        let s = format!(
            "function unused(value:{ty}):i32 {{clone(value);return 7;}} export function root(input:i32):i32 {{return input;}}"
        );
        assert!(
            matches!(claim(&[("main.zry",&s)]),Err(Failure::Diagnostics(v)) if v[0].code=="ZRYNA-M3008"),
            "{ty}"
        );
    }
}

#[test]
fn asymmetric_source_borrows_and_opaque_originals_do_not_gain_clone_capability() {
    use crate::bounded_generics_v1::{
        SemanticInput, body_types::check_body_types, resolve_declarations,
        tests::body_fixtures::project,
    };
    for ty in [
        "Borrow<Result<Option<String>,String>>",
        "Borrow<Result<String,Option<String>>>",
        "Result<Option<T>,String>",
        "Result<String,Option<T>>",
    ] {
        let generic = if ty.contains('T') { "<T extends ZrynaValue>" } else { "" };
        let header = format!("function unused{generic}(value:{ty}):i32 {{clone(value);return 7;}}");
        let mut roots = vec!["export function root(input:i32):i32 {return input;}".to_owned()];
        if ty.contains('T') {
            let closed = ty.replace('T', "String");
            let construction = if ty.starts_with("Result<Option") {
                "Result.ok<Option<String>,String>(Option.none<String>())"
            } else {
                "Result.err<String,Option<String>>(Option.none<String>())"
            };
            roots.push(format!("export function root(input:i32):i32 {{const value:{closed} ={construction};return unused<String>(value);}}"));
        }
        for root in roots {
            let s = format!("{header} {root}");
            let p = project(&[("main.zry", &s)]);
            let d = resolve_declarations(
                SemanticInput::try_new(
                    &p.syntax,
                    &p.sources,
                    p.sources.verify_file_id(0).expect("entry"),
                )
                .expect("input"),
            )
            .expect("declarations");
            assert!(check_body_types(&d).is_err(), "original capability exclusion {ty}");
        }
    }
}

#[test]
fn asymmetric_keyword_newline_unicode_and_payload_order_are_source_authenticated() {
    use crate::bounded_generics_v1::tests::body_fixtures::snapshot;
    use zryna_source::{SourceFileInput, SourceMap};
    for (ty, construction) in [
        (
            "Result<Option<String>,String>",
            "Result.ok<Option<String>,String>(Option.some<String>(\"入\"))",
        ),
        (
            "Result<String,Option<String>>",
            "Result.err<String,Option<String>>(Option.some<String>(\"入\"))",
        ),
    ] {
        let s = format!(
            "export function root(input:i32):i32 {{const value:{ty} ={construction};const copy:{ty} =clone\n(value);return input;}}"
        );
        let raw = snapshot(&[("main.zry", &s)]);
        let check = |text: &str| {
            let sources = SourceMap::build(vec![SourceFileInput {
                path: "main.zry".into(),
                text: text.into(),
            }])
            .expect("source");
            zryna_syntax::v5::verify_snapshot(raw.clone(), &sources)
        };
        claim(&[("main.zry", &s)]).expect("genuine newline and Unicode");
        check(&s).expect("original");
        for text in [
            s.replace("clone", "clоne"),
            s.replace("clone", "cl\nne"),
            s.replace('入', "出"),
            s.replace(
                ty,
                if ty.starts_with("Result<Option") {
                    "Result<String,Option<String>>"
                } else {
                    "Result<Option<String>,String>"
                },
            ),
            s.replace("clone\n", "return\n"),
        ] {
            assert!(check(&text).is_err());
            check(&s).expect("original recovery");
        }
    }
}

#[test]
fn asymmetric_frozen_source_wire_claims_reject_leaf_loan_cleanup_and_match_attacks() {
    use super::tests::with_claim;
    use zryna_ir::generic_v1::owned_v2;
    const VALUES: &str =
        include_str!("../../../../../../tests/m7-generic-owned-mixed-clone/values.zry");
    const MAIN: &str =
        include_str!("../../../../../../tests/m7-generic-owned-mixed-clone/main.zry");
    let joined = format!("{VALUES}\n{MAIN}");
    let imported = format!(
        "import {{left,right,makeLeftSome,makeLeftString,makeRightString,makeRightSome,discardLeft,discardRight}} from \"./values.zry\";\n{MAIN}"
    );
    for files in [
        vec![("main.zry", joined.as_str())],
        vec![("main.zry", imported.as_str()), ("values.zry", VALUES)],
    ] {
        with_claim(&files, |c, syntax, sources, linear, linux| {
            let abi = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
                linear,
                linux,
            )
            .expect("runtime");
            let check = |claim: &owned_v2::raw::Program| {
                owned_v2::verify(
                    owned_v2::wire::v3::decode(&owned_v2::wire::v3::encode(claim).expect("v3"))
                        .expect("decode"),
                    syntax,
                    sources,
                    sources.verify_file_id(0).expect("entry"),
                    linear,
                    linux,
                    &abi,
                )
                .map(|_| ())
            };
            check(&c).expect("pristine");
            for attack in 0..8 {
                let mut hostile = c.clone();
                super::nested_attacks::mutate_nested_claim(&mut hostile, attack);
                assert!(
                    matches!(check(&hostile), Err(Failure::Diagnostics(_))),
                    "asymmetric source-wire attack {attack}"
                );
                check(&c).expect("pristine seal after each independent mutation");
            }
        });
    }
}
