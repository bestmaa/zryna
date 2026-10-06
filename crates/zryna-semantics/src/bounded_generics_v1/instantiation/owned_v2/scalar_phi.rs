//! Required concrete scalar joins with opaque owners, hostile edges and fixed credit boundaries.
use super::tests::{claim, with_claim};
use std::fmt::Write;
use zryna_ir::generic_v1::{Failure, owned_v2, raw};

#[test]
fn concrete_scalar_branch_replacement_has_exact_join_parameters_and_edges() {
    for body in [
        "let item:i32=input; if(flag) { item=8; } return item;",
        "let item:i32=input; if(flag) { item=8; } else { item=9; } return item;",
        "let item:i32=input; if(flag) { return 7; } else { item=9; } return item;",
        "let item:i32=input; if(flag) { item=8; return item; } else { item=9; return item; }",
        "let item:i32=input; if(flag) { let item:i32=8; item=9; } return item;",
    ] {
        let source = format!("export function root(flag:bool,input:i32):i32 {{ {body} }}");
        with_claim(&[("main.zry", &source)], |c, syntax, sources, linear, linux| {
            let abi = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
                linear,
                linux,
            )
            .expect("runtime");
            let check = |c: &owned_v2::raw::Program| {
                owned_v2::verify(
                    owned_v2::wire::decode(&owned_v2::wire::encode(c).expect("wire"))
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
            check(&c).expect("source-bound scalar join");
            let function = &c.graph.functions[0];
            let joins: Vec<_> =
                function.blocks.iter().skip(1).filter(|b| !b.parameters.is_empty()).collect();
            if body.contains("return item; }") || body.contains("let item:i32=8;") {
                assert!(joins.is_empty(), "terminal or shadowed branches require no phi");
            } else {
                assert_eq!(joins.len(), 1);
                assert_eq!(joins[0].parameters.len(), 1);
                assert_eq!(joins[0].parameters[0].ty, raw::Type::Stored(1));
                let target = joins[0].id;
                let edges: Vec<_> = function
                    .blocks
                    .iter()
                    .filter_map(|b| {
                        if let raw::Terminator::Jump(edge) = &b.terminator {
                            (edge.target == target).then_some(edge)
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(edges.len(), if body.contains("return 7;") { 1 } else { 2 });
                assert!(edges.iter().all(|e| e.arguments.len() == 1));
                for mutation in 0..4 {
                    let mut hostile = c.clone();
                    let f = &mut hostile.graph.functions[0];
                    if mutation == 3 {
                        f.blocks.iter_mut().find(|b| b.id == target).expect("join").parameters[0]
                            .ty = raw::Type::Stored(0);
                    } else {
                        let edge = f
                            .blocks
                            .iter_mut()
                            .find_map(|b| {
                                if let raw::Terminator::Jump(edge) = &mut b.terminator {
                                    (edge.target == target).then_some(edge)
                                } else {
                                    None
                                }
                            })
                            .expect("incoming");
                        match mutation {
                            0 => edge.arguments.clear(),
                            1 => edge.arguments[0] = u32::MAX,
                            _ => edge.arguments[0] = 0,
                        }
                    }
                    assert!(matches!(check(&hostile), Err(Failure::Diagnostics(_))));
                    check(&c).expect("fresh recovery");
                }
            }
        });
    }
}

#[test]
fn scalar_branch_phi_composes_with_generic_results_loops_and_retained_owners() {
    for body in [
        "let code:i32=0; if(flag) { const local:String=\"左\"; code=7; } else { const local:String=\"右\"; code=9; } return Result.err<T,i32>(code);",
        "let ready:bool=true; let phase:bool=true; let code:i32=0; while(ready) { if(flag) { code=code+1; ready=false; } else { if(phase) { code=code+2; phase=false; } else { code=code+3; ready=false; } } } return Result.err<T,i32>(code);",
        "let code:i32=0; if(flag) { if(false) { code=7; } else { code=8; } } else { code=9; } if(flag) { code=code+1; } return Result.err<T,i32>(code);",
    ] {
        let source = format!(
            "function select<T extends ZrynaValue>(flag:bool,value:T):Result<T,i32> {{ {body} }} export function root(flag:bool):i32 {{ const original:String=\"λ\"; const result:Result<String,i32> =select<String>(flag,original); return match(result, {{ \"Result.ok\":(value)=>discard<String>(value), \"Result.err\":(code)=>code }}); }} function discard<T extends ZrynaValue>(value:T):i32 {{ return 0; }}"
        );
        claim(&[("main.zry", &source)])
            .expect("required scalar joins around opaque retained owners");
    }
}

#[test]
fn scalar_join_cannot_repair_opaque_owned_or_borrowed_place_state() {
    for body in [
        "let item:String=\"α\"; if(flag) { item=\"yes\"; } else { item=\"no\"; } return input;",
        "let item:Option<String> =Option.none<String>(); if(flag) { item=Option.none<String>(); } return input;",
        "let item:i32=input; const view:Borrow<i32> =borrow(item); if(flag) { item=7; } return input;",
    ] {
        let source = format!("export function root(flag:bool,input:i32):i32 {{ {body} }}");
        let Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("ownership exclusion")
        else {
            panic!("diagnostic");
        };
        assert_eq!(errors[0].code, "ZRYNA-M7007");
        assert!(errors[0].primary_span().is_some());
    }
    for called in ["bad<i32>(flag,input)", "input"] {
        let source = format!(
            "function bad<T extends ZrynaValue>(flag:bool,value:T):T {{ let item:T=value; if(flag) {{ item=item; }} return item; }} export function root(flag:bool,input:i32):i32 {{ return {called}; }}"
        );
        let Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("opaque original cannot acquire scalar phi")
        else {
            panic!("diagnostic");
        };
        assert_eq!(errors[0].code, "ZRYNA-M7007");
    }
}

#[test]
fn genuine_scalar_phi_join_credits_pin_the_unchanged_aggregate_ceiling() {
    // 401 live scalar places and801IDs initially: flag plus400 literal/Copy pairs.
    // A two-arm literal assignment adds4IDs and1phi. Six state snapshots cost
    // 6*(2*401+801+1)+12; sparse changes8, index/parameter/two-key/edge credit6.
    // Branchj costs9650+30*j; these are independently frozen counts before compilation.
    for (branches, units, accepted) in
        [(94usize, 1_038_230usize, true), (95, 1_050_700, false), (1, 9_650, true)]
    {
        assert_eq!(units, branches * 9_650 + 30 * branches * (branches - 1) / 2);
        assert_eq!(accepted, units <= 1_048_576);
        let mut source = String::from("export function root(flag:bool):i32 {");
        for i in 0..400 {
            write!(source, "let v{i}:i32=0;").expect("source");
        }
        for _ in 0..branches {
            source.push_str("if(flag){v0=7;}else{v0=9;}");
        }
        source.push_str("return v0;}");
        let result = claim(&[("main.zry", &source)]);
        if accepted {
            result.expect("fixed last accepted/recovery credit");
        } else {
            let Failure::Diagnostics(errors) = result.expect_err("fixed first extra") else {
                panic!("diagnostic");
            };
            assert_eq!(errors[0].code, "ZRYNA-M7201");
        }
    }
}

#[test]
fn scalar_phi_parameter_cap_counts_the_union_of_changed_places() {
    for (places, split, accepted) in
        [(256, false, true), (257, false, false), (257, true, false), (1, false, true)]
    {
        let mut source = String::from("export function root(flag:bool):i32 {");
        for i in 0..places {
            write!(source, "let v{i}:i32=0;").expect("source");
        }
        source.push_str("if(flag){");
        for i in 0..places {
            if !split || i % 2 == 0 {
                write!(source, "v{i}=7;").expect("source");
            }
        }
        source.push_str("}else{");
        for i in 0..places {
            if !split || i % 2 == 1 {
                write!(source, "v{i}=9;").expect("source");
            }
        }
        source.push_str("}return v0;}");
        let result = claim(&[("main.zry", &source)]);
        if accepted {
            result.expect("unchanged 256-parameter ceiling/recovery");
        } else {
            let Failure::Diagnostics(errors) = result.expect_err("first extra sparse/union place")
            else {
                panic!("diagnostic");
            };
            assert_eq!(errors[0].code, "ZRYNA-M7201");
        }
    }
}
