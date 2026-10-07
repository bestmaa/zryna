//! Finite original Copy places; opaque, owned, borrowed and loop-header exclusions.
use super::tests::{claim, with_claim};
use zryna_ir::generic_v1::{Failure, raw};

#[test]
fn concrete_copy_enum_continuations_have_exact_stable_whole_place_parameters() {
    for body in [
        "let item:Option<i32> =Option.some<i32>(13); if(flag){item=Option.none<i32>();} return 0;",
        "let item:Result<i32,bool> =Result.ok<i32,bool>(29); if(flag){item=Result.err<i32,bool>(true);}else{item=Result.ok<i32,bool>(31);} return 0;",
        "let item:Option<i32> =Option.some<i32>(13); if(flag){return 7;}else{item=Option.none<i32>();} return 0;",
    ] {
        let source = format!("export function root(flag:bool):i32 {{{body}}}");
        with_claim(&[("main.zry", &source)], |c, _, _, linear, _| {
            let f = &c.graph.functions[0];
            let join = f
                .blocks
                .iter()
                .skip(1)
                .find(|b| !b.parameters.is_empty())
                .expect("whole enum join");
            assert_eq!(join.parameters.len(), 1);
            let raw::Type::Stored(ty) = join.parameters[0].ty else {
                panic!("stored complete enum");
            };
            let view =
                linear.types().find(|t| t.id().index() == ty).expect("independent layout type");
            let expected: &[u8] = if body.contains("Result") {
                &[0x15, 2, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0]
            } else {
                &[0x14, 1, 0, 0, 0, 1, 0, 0, 0, 1]
            };
            assert_eq!(view.key(), expected);
            assert_eq!(view.drop_kind(), 0);
            let edges = f
                .blocks
                .iter()
                .filter_map(|b| match &b.terminator {
                    raw::Terminator::Jump(edge) if edge.target == join.id => Some(edge),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(edges.len(), if body.contains("return 7") { 1 } else { 2 });
            assert!(edges.iter().all(|e| e.arguments.len() == 1));
        });
    }
}

#[test]
fn finite_copy_enum_joins_do_not_grant_owned_or_additional_copy_type_capabilities() {
    for body in [
        "let item:Option<String> =Option.none<String>(); if(flag){item=Option.none<String>();} return 0;",
        "let item:Result<i32,String> =Result.ok<i32,String>(1); if(flag){item=Result.ok<i32,String>(2);} return 0;",
        "let item:Option<bool> =Option.some<bool>(true); if(flag){item=Option.none<bool>();} return 0;",
        "let item:Result<bool,i32> =Result.ok<bool,i32>(true); if(flag){item=Result.err<bool,i32>(2);} return 0;",
        "let item:Option<Option<i32>> =Option.none<Option<i32>>(); if(flag){item=Option.none<Option<i32>>();} return 0;",
        "let item:Option<i32> =Option.some<i32>(1); while(false){item=Option.none<i32>();} return 0;",
        "let item:Result<i32,bool> =Result.ok<i32,bool>(1); while(false){item=Result.err<i32,bool>(true);} return 0;",
    ] {
        let source = format!("export function root(flag:bool):i32 {{{body}}}");
        let Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("bounded capability")
        else {
            panic!("diagnostic");
        };
        assert_eq!(errors[0].code, "ZRYNA-M7007");
        assert!(errors[0].primary_span().is_some());
    }
}

#[test]
fn original_opaque_enums_reject_even_when_copy_specialized_or_unused() {
    for called in [true, false] {
        let call = if called { "bad<i32>(flag,Option.some<i32>(1))" } else { "0" };
        let source = format!(
            "function bad<T extends ZrynaValue>(flag:bool,value:Option<T>):i32 {{let item:Option<T> =value; if(flag){{item=Option.none<T>();}} return 0;}} export function root(flag:bool):i32 {{return {call};}}"
        );
        let Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("original opacity")
        else {
            panic!("diagnostic");
        };
        assert_eq!(errors[0].code, "ZRYNA-M7007");
        assert!(errors[0].primary_span().is_some());
    }
    for called in [true, false] {
        let call = if called { "bad<bool>(flag,Result.err<i32,bool>(true))" } else { "0" };
        let source = format!(
            "function bad<T extends ZrynaValue>(flag:bool,value:Result<i32,T>):i32 {{let item:Result<i32,T> =value; if(flag){{item=Result.ok<i32,T>(2);}} return 0;}} export function root(flag:bool):i32 {{return {call};}}"
        );
        let Failure::Diagnostics(errors) = claim(&[("main.zry", &source)])
            .expect_err("unused/demanded Result original cannot acquire Copy capability")
        else {
            panic!("diagnostic");
        };
        assert_eq!(errors[0].code, "ZRYNA-M7007");
        assert!(errors[0].primary_span().is_some());
    }
}

#[test]
fn borrowed_and_immutable_enum_destinations_fail_at_the_exact_assignment_token() {
    for declaration in [
        "let item:Option<i32> =Option.some<i32>(1); const view:Borrow<Option<i32>> =borrow(item);",
        "let item:Result<i32,bool> =Result.ok<i32,bool>(1); const view:BorrowMut<Result<i32,bool>> =borrowMut(item);",
        "const item:Option<i32> =Option.some<i32>(1);",
    ] {
        let replacement = if declaration.contains("Result") {
            "Result.err<i32,bool>(true)"
        } else {
            "Option.none<i32>()"
        };
        let source = format!(
            "export function root(flag:bool):i32 {{{declaration} if(flag){{item={replacement};}} return 0;}}"
        );
        let Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("destination exclusion")
        else {
            panic!("diagnostic");
        };
        assert_eq!(errors[0].code, "ZRYNA-M7007");
        let span = errors[0].primary_span().expect("exact consuming target");
        assert_eq!(&source[span.start() as usize..span.end() as usize], "item");
        assert_eq!(span.start() as usize, source.rfind("item=").expect("assignment token"));
    }
}

#[test]
fn borrowed_enum_root_rejects_before_an_independently_invalid_replacement_payload() {
    use crate::bounded_generics_v1::tests::body_fixtures::project;
    let source = "export function root(flag:bool):i32 { const marker:String=\"λ\"; let item:Option<i32> =Option.some<i32>(1); const view:Borrow<Option<i32>> =borrow(item); if(flag){item=Option.some<i32>(false);} return 0;}";
    let p = project(&[("main.zry", source)]);
    let Failure::Diagnostics(errors) =
        zryna_ir::generic_v1::owned_v2::check_original_bodies(&p.syntax, &p.sources)
            .expect_err("owning original phase checks borrowed destination before RHS")
    else {
        panic!("diagnostic");
    };
    assert_eq!(errors[0].code, "ZRYNA-M7007");
    assert!(errors[0].message.contains("borrowed original place"));
    let span = errors[0].primary_span().expect("exact UTF8 target");
    let at = source.rfind("item=").expect("assignment");
    assert_eq!(span.start() as usize, at);
    assert_eq!(span.end() as usize, at + 4);
    let control = source.replace("const view:Borrow<Option<i32>> =borrow(item);", "");
    let p = project(&[("main.zry", &control)]);
    let Failure::Diagnostics(errors) =
        zryna_ir::generic_v1::owned_v2::check_original_bodies(&p.syntax, &p.sources)
            .expect_err("invalid payload is actually rejected without the borrowed root")
    else {
        panic!("diagnostic");
    };
    assert_eq!(errors[0].code, "ZRYNA-I7001");
    assert!(errors[0].message.contains("payload differs"));
}
