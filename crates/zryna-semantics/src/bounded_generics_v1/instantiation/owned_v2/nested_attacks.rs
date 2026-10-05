//! Independent authentic source controls and forged nested v3 claims.
use super::tests::{claim, with_claim};
use zryna_ir::generic_v1::{Failure, owned_v2};

#[test]
fn nested_borrow_and_opaque_originals_do_not_acquire_clone_capability() {
    use crate::bounded_generics_v1::{
        SemanticInput, body_types::check_body_types, resolve_declarations,
        tests::body_fixtures::project,
    };
    for source in [
        "function bad(value:Borrow<Option<Option<String>>>):i32 {clone(value);return 7;} export function root(input:i32):i32 {return input;}",
        "function bad<T extends ZrynaValue>(value:Option<Option<T>>):Option<Option<T>> {return clone(value);} export function root(input:i32):i32 {return input;}",
        "function bad<T extends ZrynaValue>(value:Option<Option<T>>):Option<Option<T>> {return clone(value);} export function root(input:i32):i32 {const value:Option<Option<String>> =Option.some<Option<String>>(Option.some<String>(\"入\"));const result:Option<Option<String>> =bad<String>(value);return input;}",
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
            "original opaque/source borrow has no Clone capability"
        );
    }
}

#[test]
fn nested_clone_retains_original_and_rejects_moved_exclusive_and_temporary_sources() {
    for ty in ["Option<Option<String>>", "Result<Option<String>,Option<String>>"] {
        let good = format!(
            "function keep(value:{ty}):{ty} {{const first:{ty} =clone(value);const second:{ty} =clone(value);return value;}} export function root(input:i32):i32 {{return input;}}"
        );
        claim(&[("main.zry", &good)]).expect("original is still transferable after both clones");
        for prefix in [
            format!("const moved:{ty} =value;"),
            format!("const view:BorrowMut<{ty}> =borrowMut(value);"),
        ] {
            let source = format!(
                "function unused(value:{ty}):i32 {{{prefix} clone(value);return 7;}} export function root(input:i32):i32 {{return input;}}"
            );
            assert!(
                matches!(claim(&[("main.zry",&source)]),Err(Failure::Diagnostics(v)) if v[0].code=="ZRYNA-M7007")
            );
        }
    }
    let source = "export function root(input:i32):i32 {clone(Option.some<Option<String>>(Option.none<String>()));return input;}";
    assert!(
        matches!(claim(&[("main.zry",source)]),Err(Failure::Diagnostics(v)) if v[0].code=="ZRYNA-M7007")
    );
}

#[test]
fn nested_clone_keyword_newline_unicode_and_type_delimiters_remain_authenticated() {
    use crate::bounded_generics_v1::tests::body_fixtures::snapshot;
    use zryna_source::{SourceFileInput, SourceMap};
    let source = "export function root(input:i32):i32 {const value:Option<Option<String>> =Option.some<Option<String>>(Option.some<String>(\"入\"));const result:Option<Option<String>> =clone\n(value);return input;}";
    let raw = snapshot(&[("main.zry", source)]);
    let check = |text: &str| {
        let sources =
            SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text: text.into() }])
                .expect("source");
        zryna_syntax::v5::verify_snapshot(raw.clone(), &sources)
    };
    check(source).expect("original bytes");
    claim(&[("main.zry", source)]).expect("genuine nested newline/Unicode");
    for text in [
        source.replace("clone", "clоne"),
        source.replace("clone", "cl\nne"),
        source.replace('入', "出"),
        source.replace("Option<Option<String>>", "Option<Option<String> >"),
        source.replace("Option<Option<String>>", "Option<Result<String>>"),
    ] {
        assert!(check(&text).is_err());
        check(source).expect("original after each hostile byte change");
    }
}

#[test]
fn nested_source_wire_attacks_reject_in_both_module_forms_with_pristine_recovery() {
    let helper = "export function keep<T extends ZrynaValue>(value:Option<Option<String>>):i32 {const first:Option<Option<String>> =clone(value);const second:Option<Option<String>> =clone(value);return 7;}";
    let main = "export function root(input:i32):i32 {const value:Option<Option<String>> =Option.some<Option<String>>(Option.some<String>(\"入\"));return keep<i32>(value);}";
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
            let check = |c: &owned_v2::raw::Program| {
                owned_v2::verify(
                    owned_v2::wire::v3::decode(&owned_v2::wire::v3::encode(c).expect("v3"))
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
            check(&claim).expect("pristine");
            for attack in 0..8 {
                let mut hostile = claim.clone();
                mutate_nested_claim(&mut hostile, attack);
                assert!(
                    matches!(check(&hostile), Err(Failure::Diagnostics(_))),
                    "nested attack{attack}"
                );
                check(&claim).expect("pristine after every independent mutation");
            }
        });
    }
}

pub(super) fn mutate_nested_claim(hostile: &mut owned_v2::raw::Program, attack: usize) {
    let clone = hostile
        .extensions
        .iter_mut()
        .flatten()
        .find(|e| matches!(e.operation, owned_v2::raw::Operation::CloneBorrowedString(_)))
        .expect("leaf");
    let result = clone.result;
    match attack {
        0 => clone.operation = owned_v2::raw::Operation::CloneBorrowedString(0),
        1 => clone.operation = owned_v2::raw::Operation::CloneBorrowedString(result),
        2 => clone.operation = owned_v2::raw::Operation::CloneBorrowedString(u32::MAX),
        3 => hostile
            .plans
            .iter_mut()
            .flat_map(|p| &mut p.steps)
            .find(|s| s.failure && s.end_loans.len() >= 3)
            .expect("nested cleanup")
            .end_loans
            .reverse(),
        4 => {
            hostile
                .plans
                .iter_mut()
                .flat_map(|p| &mut p.steps)
                .find(|s| s.failure && s.end_loans.len() >= 3)
                .expect("nested cleanup")
                .end_loans
                .remove(1);
        }
        5 => {
            hostile
                .plans
                .iter_mut()
                .flat_map(|p| &mut p.steps)
                .find(|s| s.failure && s.end_loans.len() >= 3)
                .expect("before result")
                .cleanup
                .insert(0, result);
        }
        6 => {
            let end = hostile
                .extensions
                .iter_mut()
                .flatten()
                .find(|e| matches!(e.operation, owned_v2::raw::Operation::EndLoan(_)))
                .expect("end");
            end.operation = owned_v2::raw::Operation::EndLoan(0);
        }
        _ => {
            let term = hostile
                .graph
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.blocks)
                .filter_map(|b| match &mut b.terminator {
                    zryna_ir::generic_v1::raw::Terminator::ClosedEnumMatch { mode, .. } => {
                        Some(mode)
                    }
                    _ => None,
                })
                .nth(1)
                .expect("inner selection");
            *term = zryna_ir::generic_v1::raw::MatchMode::Value;
        }
    }
}
