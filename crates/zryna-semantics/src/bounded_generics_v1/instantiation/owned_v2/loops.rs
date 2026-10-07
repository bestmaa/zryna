//! Genuine loop source, exact opaque states and independently attacked backedge claims.
use super::tests::{claim, with_claim};
use zryna_ir::generic_v1::{Failure, owned_v2, raw};

#[test]
fn structured_while_execution_fixtures_seal_in_both_module_forms() {
    const VALUES: &str = include_str!("../../../../../../tests/m7-generic-owned-while/values.zry");
    const MAIN: &str = include_str!("../../../../../../tests/m7-generic-owned-while/main.zry");
    for cross in [false, true] {
        let joined = format!("{VALUES}\n{MAIN}");
        let imported = format!("import {{probe,flip,keep,discard}} from \"./values.zry\";\n{MAIN}");
        let files = if cross {
            vec![("main.zry", imported.as_str()), ("values.zry", VALUES)]
        } else {
            vec![("main.zry", joined.as_str())]
        };
        if let Some(dir) = std::env::var_os("ZRYNA_GENERIC_OWNED_WHILE_FIXTURE_DIR") {
            std::fs::write(
                std::path::PathBuf::from(dir).join(format!("{}-reference.json", files.len())),
                serde_json::to_vec_pretty(
                    &crate::bounded_generics_v1::tests::body_fixtures::snapshot(&files),
                )
                .expect("independent DTO"),
            )
            .expect("explicit independently read fixture output");
        }
        with_claim(&files, |claim, syntax, sources, linear, linux| {
            let runtime = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
                linear,
                linux,
            )
            .expect("runtime");
            let check = |claim: &owned_v2::raw::Program| {
                owned_v2::verify(
                    owned_v2::wire::decode(&owned_v2::wire::encode(claim).expect("wire"))
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
            check(&claim).expect("exact loop source, CFG, ownership and cleanup");
            for mutation in 0..8 {
                let mut hostile = claim.clone();
                attack(&mut hostile, mutation);
                assert!(matches!(check(&hostile), Err(Failure::Diagnostics(_))));
                check(&claim).expect("pristine recovery after each independent mutation");
            }
        });
    }
}

fn attack(hostile: &mut owned_v2::raw::Program, mutation: usize) {
    match mutation {
        0..=2 | 6 | 7 => {
            let edge = hostile
                .graph
                .functions
                .iter_mut()
                .find(|f| f.public_export.as_deref() == Some("looped"))
                .expect("loop export")
                .blocks
                .iter_mut()
                .enumerate()
                .find_map(|(index, b)| match &mut b.terminator {
                    raw::Terminator::Jump(edge)
                        if edge.target as usize <= index && !edge.arguments.is_empty() =>
                    {
                        Some(edge)
                    }
                    _ => None,
                })
                .expect("actual scalar backedge");
            match mutation {
                0 => {
                    edge.arguments.pop();
                }
                1 => edge.arguments[0] = u32::MAX,
                2 => edge.target = 0,
                6 => edge.arguments.swap(0, 2),
                _ => edge.arguments[0] = edge.arguments[1],
            }
        }
        3 => {
            let branch = hostile
                .graph
                .functions
                .iter_mut()
                .find(|f| f.public_export.as_deref() == Some("looped"))
                .expect("loop export")
                .blocks
                .iter_mut()
                .find_map(|b| match &mut b.terminator {
                    raw::Terminator::Branch { yes, no, .. } => Some((yes, no)),
                    _ => None,
                })
                .expect("loop condition branch");
            std::mem::swap(branch.0, branch.1);
        }
        4 => {
            hostile
                .extensions
                .iter_mut()
                .flatten()
                .find(|e| matches!(e.operation, owned_v2::raw::Operation::EndLoan(_)))
                .expect("lexical loan cleanup")
                .operation = owned_v2::raw::Operation::EndLoan(u32::MAX);
        }
        _ => hostile
            .plans
            .iter_mut()
            .flat_map(|p| &mut p.steps)
            .find(|s| s.failure && !s.cleanup.is_empty())
            .expect("failure cleanup")
            .cleanup
            .clear(),
    }
}

#[test]
fn used_and_unused_opaque_loop_moves_fail_before_copy_specialization() {
    for call in ["bad<i32>(false,input)", "input"] {
        let source = format!(
            "function bad<T extends ZrynaValue>(flag:bool,value:T):T {{ while(flag) {{ const taken:T=value; }} return value; }} export function root(input:i32):i32 {{ return {call}; }}"
        );
        let Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("opaque backedge")
        else {
            panic!("source diagnostic")
        };
        assert_eq!(errors[0].code, "ZRYNA-M7007");
        let span = errors[0].primary_span().expect("authenticated loop witness");
        assert!(source[span.start() as usize..span.end() as usize].starts_with("while(flag)"));
    }
}

#[test]
fn condition_consumption_incoming_loan_changes_and_borrowed_header_roots_reject() {
    for body in [
        "const item:String=\"α\"; while(test(item)) {} return 7;",
        "const item:String=\"α\"; const view:Borrow<String> =borrow(item); while(flag) { view; } return 7;",
        "let count:i32=7; const view:Borrow<i32> =borrow(count); while(flag) {} return count;",
        "let item:Option<i32> =Option.none<i32>(); while(flag) { item=Option.some<i32>(7); } return 7;",
    ] {
        let source = format!(
            "function test(value:String):bool {{ return false; }} export function root(flag:bool):i32 {{ {body} }}"
        );
        let Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("finite loop exclusion")
        else {
            panic!("source diagnostic")
        };
        assert_eq!(errors[0].code, "ZRYNA-M7007");
        let span = errors[0].primary_span().expect("exact while witness");
        assert!(source[span.start() as usize..span.end() as usize].starts_with("while("));
    }
}

#[test]
fn direct_terminal_body_can_move_opaque_owner_without_a_backedge() {
    let source = "function terminal<T extends ZrynaValue>(flag:bool,value:T):T { while(flag) { return value; } return value; } export function root(flag:bool):i32 { return terminal<i32>(flag,7); }";
    claim(&[("main.zry", source)]).expect("only continuing paths require header restoration");
}

#[test]
fn matched_condition_and_asymmetric_return_preserve_repeated_header_and_false_exit() {
    let source = "export function root(flag:bool):i32 { const outer:String=\"α\"; let go:bool=true; while(match(Option.some<bool>(go), {\"Option.some\":(value)=>value,\"Option.none\":()=>false})) { const local:String=\"body\"; if(flag) { return 11; } go=false; } return 13; }";
    with_claim(&[("main.zry", source)], |claim, syntax, sources, linear, linux| {
        let runtime = zryna_ownership_runtime_abi::generic_v1::verify_v1(
            zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
            linear,
            linux,
        )
        .expect("runtime");
        owned_v2::verify(
            owned_v2::wire::decode(&owned_v2::wire::encode(&claim).expect("wire")).expect("decode"),
            syntax,
            sources,
            sources.verify_file_id(0).expect("entry"),
            linear,
            linux,
            &runtime,
        )
        .expect("condition's internal CFG is preserved, terminal arm and backedge both seal");
    });
}

#[test]
fn genuine_loop_state_amplification_rejects_first_extra_and_recovers() {
    use std::fmt::Write;
    // 401 immutable one-byte bindings and availability entries plus one frame:
    // 1204 units per complete snapshot, two per loop; 435 fit, 436 cannot.
    for loops in [435, 436, 1] {
        let mut source = "export function root(flag:bool):i32 {\n".to_owned();
        for index in 0..400 {
            writeln!(source, "const local{index}:i32=0;").expect("source");
        }
        for _ in 0..loops {
            source.push_str("while(flag) {}\n");
        }
        source.push_str("return 7; }");
        let result = claim(&[("main.zry", &source)]);
        if loops == 436 {
            let Failure::Diagnostics(errors) = result.expect_err("first excess state credit")
            else {
                panic!("budget diagnostic")
            };
            assert_eq!(errors[0].code, "ZRYNA-M7201");
        } else {
            result.expect("largest whole-loop count or pristine recovery");
        }
    }
}

#[test]
fn scalar_header_arity_is_exact_and_first_extra_fails_before_allocation() {
    use std::fmt::Write;
    for places in [256, 257, 1] {
        let mut source = "export function root(flag:bool):i32 {\n".to_owned();
        for index in 0..places {
            writeln!(source, "let local{index}:i32=0;").expect("source");
        }
        source.push_str("while(flag) {} return 7; }");
        let result = claim(&[("main.zry", &source)]);
        if places == 257 {
            let Failure::Diagnostics(errors) = result.expect_err("first extra header place") else {
                panic!("budget diagnostic")
            };
            assert_eq!(errors[0].code, "ZRYNA-M7201");
        } else {
            result.expect("exact scalar header arity or pristine recovery");
        }
    }
}

#[test]
fn while_keyword_newline_and_unicode_authentication_remain_mandatory() {
    use crate::bounded_generics_v1::tests::body_fixtures::snapshot;
    use zryna_source::{SourceFileInput, SourceMap};
    use zryna_syntax::v5::verify_snapshot;
    let source = "export function root(flag:bool):i32 { const item:String=\"α\"; let count:i32=7; while\n(flag) { count=8; } return count; }";
    let raw = snapshot(&[("main.zry", source)]);
    let check = |text: &str| {
        let sources =
            SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text: text.into() }])
                .expect("source");
        verify_snapshot(raw.clone(), &sources)
    };
    check(source).expect("ordinary whitespace before loop condition is authenticated");
    claim(&[("main.zry", source)]).expect("genuine loop with UTF8 owner and newline seals");
    for text in [
        source.replace("while", "whilx"),
        source.replace("while", "whiℓe"),
        source.replace('α', "β"),
    ] {
        assert!(check(&text).is_err(), "forged unchanged DTO cannot authenticate changed bytes");
        check(source).expect("pristine syntax recovery");
    }
    let text = source.replace("return count", "return\ncount");
    let raw = snapshot(&[("main.zry", &text)]);
    let sources =
        SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text }]).expect("source");
    assert!(
        verify_snapshot(raw, &sources).is_err(),
        "return newline cannot forge executable return"
    );
}
