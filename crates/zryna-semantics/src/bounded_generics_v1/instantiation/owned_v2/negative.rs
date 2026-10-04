//! Genuine independent source negatives: owning phase, exact byte range, no partial seal.
use super::tests::claim;
use zryna_ir::generic_v1::Failure;
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
