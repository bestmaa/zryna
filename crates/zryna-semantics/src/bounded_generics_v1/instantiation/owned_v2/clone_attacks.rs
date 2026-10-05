//! Genuine source exclusions, byte authentication and independently forged v3 claims.
use super::tests::{claim, with_claim};
use std::fmt::Write;
use zryna_ir::generic_v1::{Failure, owned_v2};

#[test]
fn structural_clone_preserves_source_owner_and_surrounding_shared_loans() {
    let source = "function keep(value:Option<String>):Option<String> { const first:Option<String> =clone(value); const second:Option<String> =clone(value); return value; } export function root(input:i32):i32 { const value:Result<String,String> =Result.err<String,String>(\"失敗\"); const view:Borrow<Result<String,String>> =borrow(value); const first:Result<String,String> =clone(value); const second:Result<String,String> =clone(value); return input; }";
    claim(&[("main.zry", source)]).expect("repeat clones retain source and surrounding loans");
}

#[test]
fn structural_clone_rejects_moved_exclusively_borrowed_and_unsupported_owners() {
    for (prefix, ty, expected) in [
        ("const taken:Option<String> =value;", "Option<String>", "ZRYNA-M7007"),
        (
            "const view:BorrowMut<Option<String>> =borrowMut(value);",
            "Option<String>",
            "ZRYNA-M7007",
        ),
        ("", "Option<i32>", "ZRYNA-M3008"),
        ("", "Option<Option<Option<String>>>", "ZRYNA-M3008"),
        ("", "Result<String,bool>", "ZRYNA-M3008"),
    ] {
        let source = format!(
            "function unused(value:{ty}):i32 {{ {prefix} clone(value); return 7; }} export function root(input:i32):i32 {{ return input; }}"
        );
        let Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("original source exclusion")
        else {
            panic!("source diagnostic")
        };
        assert_eq!(errors[0].code, expected, "{source}");
        if expected == "ZRYNA-M7007" {
            let span = errors[0].primary_span().expect("authenticated ownership witness");
            assert!(source[span.start() as usize..span.end() as usize].contains("value"));
        }
    }
}

#[test]
fn source_clone_borrow_and_opaque_specialization_remain_body_type_errors() {
    use crate::bounded_generics_v1::{
        SemanticInput, body_types::check_body_types, resolve_declarations,
        tests::body_fixtures::project,
    };
    for source in [
        "function unused(value:Borrow<String>):i32 { clone(value); return 7; } export function root(input:i32):i32 { return input; }",
        "function bad<T extends ZrynaValue>(value:T):T { return clone(value); } export function root(input:i32):i32 { return bad<i32>(input); }",
        "function bad<T extends ZrynaValue>(value:T):T { return clone(value); } export function root(input:i32):i32 { return input; }",
    ] {
        let p = project(&[("main.zry", source)]);
        let d = resolve_declarations(
            SemanticInput::try_new(
                &p.syntax,
                &p.sources,
                p.sources.verify_file_id(0).expect("entry"),
            )
            .expect("input"),
        )
        .expect("declarations");
        assert!(
            check_body_types(&d).is_err(),
            "specialization/unused source cannot create Clone capability"
        );
    }
}

#[test]
fn clone_keyword_newline_and_unicode_bytes_remain_authenticated() {
    use crate::bounded_generics_v1::tests::body_fixtures::snapshot;
    use zryna_source::{SourceFileInput, SourceMap};
    use zryna_syntax::v5::verify_snapshot;
    let source = "export function root(input:i32):i32 { const value:Option<String> =Option.some<String>(\"入\"); const result:Option<String> =clone\n(value); return input; }";
    let raw = snapshot(&[("main.zry", source)]);
    let check = |text: &str| {
        let sources =
            SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text: text.into() }])
                .expect("source");
        verify_snapshot(raw.clone(), &sources)
    };
    check(source).expect("authenticated ordinary whitespace");
    claim(&[("main.zry", source)]).expect("genuine clone newline and Unicode payload");
    for text in [
        source.replace("clone", "clonx"),
        source.replace("clone", "clоne"),
        source.replace('入', "出"),
        source.replace("clone", "cl\nne"),
    ] {
        assert!(check(&text).is_err(), "unchanged DTO cannot authenticate changed bytes");
        check(source).expect("pristine source recovery");
    }
}

#[test]
fn independent_source_wire_attacks_cannot_change_selected_clone_or_cleanup() {
    let helper = "export function keep<T extends ZrynaValue>(value:Option<String>):i32 { const first:Option<String> =clone(value); const second:Option<String> =clone(value); return 7; }";
    let main = "export function root(input:i32):i32 { const value:Option<String> =Option.some<String>(\"入\"); return keep<i32>(value); }";
    let joined = format!("{helper}\n{main}");
    let imported = format!("import {{keep}} from \"./values.zry\";\n{main}");
    for files in [
        vec![("main.zry", joined.as_str())],
        vec![("main.zry", imported.as_str()), ("values.zry", helper)],
    ] {
        with_claim(&files, |claim, syntax, sources, linear, linux| {
            let runtime = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
                linear,
                linux,
            )
            .expect("runtime");
            let check = |claim: &owned_v2::raw::Program| {
                owned_v2::verify(
                    owned_v2::wire::v3::decode(&owned_v2::wire::v3::encode(claim).expect("raw v3"))
                        .expect("decode"),
                    syntax,
                    sources,
                    sources.verify_file_id(0).expect("entry"),
                    linear,
                    linux,
                    &runtime,
                )
                .map(|_| ())
            };
            check(&claim).expect("pristine seal");
            for attack in 0..7 {
                let mut hostile = claim.clone();
                let clone = hostile
                    .extensions
                    .iter_mut()
                    .flatten()
                    .find(|e| {
                        matches!(e.operation, owned_v2::raw::Operation::CloneBorrowedString(_))
                    })
                    .expect("selected clone");
                match attack {
                    0 => clone.operation = owned_v2::raw::Operation::CloneString(0),
                    1 => clone.operation = owned_v2::raw::Operation::CloneBorrowedString(u32::MAX),
                    2 => {
                        let id = clone.result;
                        clone.operation = owned_v2::raw::Operation::CloneBorrowedString(id);
                    }
                    3 => {
                        let id = clone.result;
                        let step = hostile
                            .plans
                            .iter_mut()
                            .flat_map(|p| &mut p.steps)
                            .find(|s| s.failure && !s.end_loans.is_empty())
                            .expect("clone failure snapshot");
                        step.cleanup.insert(0, id);
                    }
                    4 => {
                        hostile
                            .plans
                            .iter_mut()
                            .flat_map(|p| &mut p.steps)
                            .find(|s| s.failure && !s.end_loans.is_empty())
                            .expect("loan cleanup")
                            .end_loans
                            .clear();
                    }
                    5 => {
                        hostile
                            .plans
                            .iter_mut()
                            .flat_map(|p| &mut p.steps)
                            .find(|s| s.failure && s.cleanup.len() > 1)
                            .expect("ordered cleanup")
                            .cleanup
                            .reverse();
                    }
                    _ => {
                        let term = hostile.graph.functions[0]
                            .blocks
                            .iter_mut()
                            .find_map(|b| match &mut b.terminator {
                                zryna_ir::generic_v1::raw::Terminator::ClosedEnumMatch {
                                    mode,
                                    ..
                                } => Some(mode),
                                _ => None,
                            })
                            .expect("shared selection");
                        *term = zryna_ir::generic_v1::raw::MatchMode::Value;
                    }
                }
                assert!(
                    matches!(check(&hostile), Err(Failure::Diagnostics(_))),
                    "source wire attack {attack}"
                );
                check(&claim).expect("pristine seal after every attack");
            }
        });
    }
}

#[test]
fn structural_clone_requires_one_named_complete_owner() {
    let source =
        "export function root(input:i32):i32 { clone(Option.some<String>(\"入\")); return input; }";
    let Failure::Diagnostics(errors) = claim(&[("main.zry", source)])
        .expect_err("temporary cannot create hidden owner observation")
    else {
        panic!("ownership diagnostic")
    };
    assert_eq!(errors[0].code, "ZRYNA-M7007");
}

#[test]
fn result_snapshot_credit_rejects_first_extra_and_recovers() {
    let mut body = String::new();
    for i in 0..400 {
        write!(body, "const p{i}:i32=0;").expect("source fixture");
    }
    // Fifteen root-key bytes and twelve definitions/clone:5028+48*j credits.
    // 129 clones=1044900;130=1056120, over the unchanged1048576 ceiling.
    for count in [129, 130, 1] {
        let source = format!(
            "function churn(value:Result<String,String>):i32 {{ {body} {} return 0; }} export function root(input:i32):i32 {{return input;}}",
            "clone(value);".repeat(count)
        );
        let result = claim(&[("main.zry", &source)]);
        if count == 130 {
            assert!(matches!(result,Err(Failure::Diagnostics(v)) if v[0].code=="ZRYNA-M7201"));
        } else {
            result.expect("exact/recovery complete Result snapshot credit");
        }
    }
}
