//! Genuine independent source negatives: owning phase, exact byte range, no partial seal.
use super::tests::claim;
use zryna_ir::generic_v1::Failure;

#[test]
fn concat_intrinsic_remains_excluded_from_owned_execution() {
    let failure = claim(&[(
        "main.zry",
        "export function root(input:i32):String {return concat(\"left\",\"right\");}",
    )])
    .expect_err("metadata concat admission cannot issue an owned execution context");
    let Failure::Diagnostics(errors) = failure else { panic!("owning admission diagnostic") };
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "ZRYNA-M3008");
}
#[test]
fn opaque_copy_specialization_never_legalizes_repeated_original_move() {
    for source in [
        "function bad<T extends ZrynaValue>(value:T):T { const once:T=value; return value; } export function root(input:i32):i32 { return bad<i32>(input); }",
        "function bad<T extends ZrynaValue>(value:T):T { const once:T=value; return value; } export function root(input:i32):i32 { return input; }",
    ] {
        let failure = claim(&[("main.zry", source)])
            .expect_err("closed Copy/unused template cannot repair opaque owner affinity");
        let Failure::Diagnostics(errors) = failure else { panic!("owning source diagnostic") };
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "ZRYNA-M7007");
        let at = source.find("return value").expect("source witness") + 7;
        let span = errors[0].primary_span().expect("authenticated invalid consuming token");
        assert_eq!(span.start() as usize, at);
        assert_eq!(span.end() as usize, at + 5);
    }
}
#[test]
fn live_shared_loan_blocks_move_and_exclusive_loan() {
    for operation in
        ["const taken:String=item;", "const exclusive:BorrowMut<String> =borrowMut(item);"]
    {
        let source = format!(
            "export function root(input:i32):i32 {{ const item:String=\"α\"; const view:Borrow<String> =borrow(item); {operation} return input; }}"
        );
        let failure =
            claim(&[("main.zry", &source)]).expect_err("overlapping ownership source operation");
        let Failure::Diagnostics(errors) = failure else { panic!("owning source diagnostic") };
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "ZRYNA-M7007");
        let span = errors[0].primary_span().expect("exact UTF8 source range");
        let witness = &source[span.start() as usize..span.end() as usize];
        assert!(witness == "item" || witness == "borrowMut(item)", "{witness}");
    }
}
#[test]
fn one_call_cannot_duplicate_an_exclusive_payload_alias() {
    let source = "function see(a:BorrowMut<String>,b:BorrowMut<String>):i32 { return 7; } export function root(input:i32):i32 { const item:String=\"α\"; const view:BorrowMut<String> =borrowMut(item); return see(view,view); }";
    let Failure::Diagnostics(errors) =
        claim(&[("main.zry", source)]).expect_err("two exclusive aliases")
    else {
        panic!("source diagnostic")
    };
    assert_eq!(errors[0].code, "ZRYNA-M7007");
    let span = errors[0].primary_span().expect("source alias call");
    assert_eq!(&source[span.start() as usize..span.end() as usize], "see(view,view)");
}
#[test]
fn unsupported_whole_container_types_fail_before_discovery_and_layout() {
    for ty in ["Vec<i32>", "Borrow<Vec<i32>>", "Option<Vec<i32>>", "Result<i32,Vec<String>>"] {
        let source = format!(
            "function unused(value:{ty}):i32 {{ return 7; }} export function root(input:i32):i32 {{ return input; }}"
        );
        let Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("unsupported container original")
        else {
            panic!("source admission diagnostic before discovery/layout")
        };
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "ZRYNA-M3008");
        assert!(errors[0].message.contains("containers need a successor ownership proof"));
    }
}
#[test]
fn original_loan_returns_fail_at_the_exact_escaping_expression_before_layout() {
    for signature in [
        "function escape(value:Borrow<String>):Borrow<String>",
        "function escape(value:BorrowMut<String>):BorrowMut<String>",
        "function escape<T extends ZrynaValue>(value:Borrow<T>):Borrow<T>",
    ] {
        let source = format!(
            "{signature} {{ return value; }} export function root(input:i32):i32 {{ return input; }}"
        );
        let Failure::Diagnostics(errors) =
            claim(&[("main.zry", &source)]).expect_err("unused original cannot return a loan")
        else {
            panic!("source ownership diagnostic before layout")
        };
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "ZRYNA-M7007");
        let span = errors[0].primary_span().expect("exact escaping operation span");
        assert_eq!(&source[span.start() as usize..span.end() as usize], "value");
    }
}
