//! Genuine structured source and independently mutated raw branch/cleanup claims.
use super::tests::{claim, with_claim};
use zryna_ir::generic_v1::{Failure, owned_v2, raw};

#[test]
fn structured_branch_execution_fixtures_seal_in_both_module_forms() {
    const VALUES: &str =
        include_str!("../../../../../../tests/m7-generic-owned-branches/values.zry");
    const MAIN: &str = include_str!("../../../../../../tests/m7-generic-owned-branches/main.zry");
    for cross in [false, true] {
        let joined = format!("{VALUES}\n{MAIN}");
        let imported = format!("import {{discard,select}} from \"./values.zry\";\n{MAIN}");
        let files = if cross {
            vec![("main.zry", imported.as_str()), ("values.zry", VALUES)]
        } else {
            vec![("main.zry", joined.as_str())]
        };
        if let Some(dir) = std::env::var_os("ZRYNA_GENERIC_OWNED_BRANCH_FIXTURE_DIR") {
            std::fs::write(
                std::path::PathBuf::from(dir).join(format!("{}-reference.json", files.len())),
                serde_json::to_vec_pretty(
                    &crate::bounded_generics_v1::tests::body_fixtures::snapshot(&files),
                )
                .expect("DTO"),
            )
            .expect("explicit independently read source fixture output");
        }
        with_claim(&files, |claim, syntax, sources, linear, linux| {
            let runtime = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
                linear,
                linux,
            )
            .expect("runtime");
            let p = owned_v2::verify(
                owned_v2::wire::decode(&owned_v2::wire::encode(&claim).expect("wire"))
                    .expect("decode"),
                syntax,
                sources,
                sources.verify_file_id(0).expect("entry"),
                linear,
                linux,
                &runtime,
            )
            .expect("exact source and plan");
            assert_eq!(p.scalar_abi().exports().len(), 3);
        });
    }
}

#[test]
fn structured_branches_preserve_exact_incoming_owners_and_direct_terminal_returns() {
    let source = "function select<T extends ZrynaValue>(flag:bool,value:T):T { if(flag) { return value; } else { return value; } } export function root(flag:bool):i32 { const item:Option<Result<String,String>> =Option.some<Result<String,String>>(Result.ok<String,String>(\"α\")); if(flag) { const local:String=\"yes\"; if(false) { const nested:String=\"nested\"; } } else { const local:String=\"no\"; } if(flag) { return 7; } const result:i32=select<i32>(flag,8); return result; }";
    with_claim(&[("main.zry", source)], |claim, syntax, sources, linear, linux| {
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
        check(&claim).expect("independent exact source and owner/loan plan");
        let terminal = &claim.graph.functions[0];
        assert_eq!(terminal.blocks.len(), 3);
        assert!(
            terminal.blocks[1..].iter().all(|b| matches!(b.terminator, raw::Terminator::Return(_)))
        );
        for mutation in 0..5 {
            let mut hostile = claim.clone();
            match mutation {
                0 | 1 => {
                    let branch = hostile
                        .graph
                        .functions
                        .iter_mut()
                        .flat_map(|f| &mut f.blocks)
                        .find_map(|b| {
                            if let raw::Terminator::Branch { yes, no, .. } = &mut b.terminator {
                                Some((yes, no))
                            } else {
                                None
                            }
                        })
                        .expect("branch");
                    if mutation == 0 {
                        std::mem::swap(branch.0, branch.1);
                    } else {
                        branch.0.arguments.push(u32::MAX);
                    }
                }
                2 => {
                    hostile
                        .extensions
                        .iter_mut()
                        .flatten()
                        .find(|e| matches!(e.operation, owned_v2::raw::Operation::Drop(_)))
                        .expect("scope drop")
                        .operation = owned_v2::raw::Operation::Drop(u32::MAX);
                }
                3 => {
                    hostile
                        .plans
                        .iter_mut()
                        .flat_map(|p| &mut p.steps)
                        .find(|s| s.failure && !s.cleanup.is_empty())
                        .expect("branch fault cleanup")
                        .cleanup
                        .clear();
                }
                _ => {
                    let condition = hostile
                        .graph
                        .functions
                        .iter_mut()
                        .flat_map(|f| &mut f.blocks)
                        .find_map(|b| {
                            if let raw::Terminator::Branch { condition, .. } = &mut b.terminator {
                                Some(condition)
                            } else {
                                None
                            }
                        })
                        .expect("condition");
                    *condition = u32::MAX;
                }
            }
            assert!(matches!(check(&hostile), Err(Failure::Diagnostics(_))));
            check(&claim).expect("pristine recovery after each independent raw attack");
        }
    });
}

#[test]
fn opaque_branch_moves_are_checked_before_copy_substitution_in_used_and_unused_originals() {
    for call in ["bad<i32>(true,input)", "input"] {
        let source = format!(
            "function bad<T extends ZrynaValue>(flag:bool,value:T):T {{ if(flag) {{ const taken:T=value; }} return value; }} export function root(input:i32):i32 {{ return {call}; }}"
        );
        let Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("opaque mismatch")
        else {
            panic!("source diagnostic")
        };
        assert_eq!(errors[0].code, "ZRYNA-M7007");
        assert!(errors[0].message.contains("state differs"));
        let span = errors[0].primary_span().expect("authenticated if witness");
        assert!(source[span.start() as usize..span.end() as usize].starts_with("if(flag)"));
    }
    let source = "function bad<T extends ZrynaValue>(flag:bool,value:T):T { if(flag) { const taken:T=value; return value; } else { return value; } } export function root(input:i32):i32 { return input; }";
    let Failure::Diagnostics(errors) =
        claim(&[("main.zry", source)]).expect_err("unused repeated move")
    else {
        panic!("source diagnostic")
    };
    assert_eq!(errors[0].code, "ZRYNA-M7007");
    let span = errors[0].primary_span().expect("exact repeated token");
    assert_eq!(span.start() as usize, source.find("return value").expect("witness") + 7);
}

#[test]
fn unequal_loan_states_and_branch_replacement_fail_without_implicit_repair() {
    for body in [
        "const item:String=\"α\"; const view:Borrow<String> =borrow(item); if(flag) { view; } return input;",
        "let item:String=\"α\"; if(flag) { item=\"yes\"; } else { item=\"no\"; } return input;",
        "let item:i32=input; if(flag) { item=8; } return item;",
    ] {
        let source = format!("export function root(flag:bool,input:i32):i32 {{ {body} }}");
        let Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("bounded no-phi exclusion")
        else {
            panic!("source diagnostic")
        };
        assert_eq!(errors[0].code, "ZRYNA-M7007");
        let span = errors[0].primary_span().expect("exact branch witness");
        assert!(source[span.start() as usize..span.end() as usize].starts_with("if(flag)"));
    }
}

#[test]
fn genuine_branch_state_amplification_stops_at_first_extra_and_recovers() {
    use std::fmt::Write;
    // 401 places, availability entries and one-byte type keys plus one frame: 1204 units
    // per snapshot, six snapshots per empty if. 145 fit the 1,048,576-unit ceiling.
    for branches in [145, 146, 1] {
        let mut source = "export function root(flag:bool):i32 {\n".to_owned();
        for index in 0..400 {
            writeln!(source, "const local{index}:i32=0;").expect("source");
        }
        for _ in 0..branches {
            source.push_str("if(flag) {}\n");
        }
        source.push_str("return 7; }");
        let result = claim(&[("main.zry", &source)]);
        if branches == 146 {
            let Failure::Diagnostics(errors) =
                result.expect_err("first excess snapshot before discovery/layout")
            else {
                panic!("budget diagnostic")
            };
            assert_eq!(errors[0].code, "ZRYNA-M7201");
        } else {
            result.expect("exact/source recovery without partial state");
        }
    }
}

#[test]
fn nested_binding_key_bytes_cannot_hide_inside_one_place_credit() {
    use std::fmt::Write;
    let inner = format!("{}i32{}", "Option<".repeat(7), ">".repeat(7));
    // One bool key and 100 eight-layer (73-byte) Option keys. Each of six snapshots
    // charges 7,504 units. 23 branches fit; the 24th cannot copy another complete state.
    for branches in [23, 24] {
        let mut source = "export function root(flag:bool):i32 {\n".to_owned();
        for index in 0..100 {
            writeln!(source, "const local{index}:Option<{inner}> =Option.none<{inner}>();")
                .expect("source");
        }
        for _ in 0..branches {
            source.push_str("if(flag) {}\n");
        }
        source.push_str("return 7; }");
        let result = claim(&[("main.zry", &source)]);
        if branches == 24 {
            let Failure::Diagnostics(errors) = result.expect_err("copied key bytes are bounded")
            else {
                panic!("budget diagnostic")
            };
            assert_eq!(errors[0].code, "ZRYNA-M7201");
        } else {
            result.expect("exact largest whole-state count with nested keys");
        }
    }
}
