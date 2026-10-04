//! Genuine bounded source witnesses for materialization ceilings, without unbounded allocation.
use super::tests::claim;
use zryna_ir::generic_v1::Failure;

#[test]
fn executable_literal_ceiling_is_checked_before_copying_and_unused_templates_keep_no_bytes() {
    for size in [65_536, 65_537] {
        let literal = "x".repeat(size);
        let source = format!(
            "export function root(input:i32):i32 {{ const owner:String=\"{literal}\"; return input; }}"
        );
        let result = claim(&[("main.zry", &source)]);
        if size == 65_536 {
            let owned = result.expect("exact admitted executable literal");
            assert!(owned.extensions.iter().flatten().any(|e| matches!(
                &e.operation,
                zryna_ir::generic_v1::owned_v2::raw::Operation::StringLiteral(bytes)
                    if bytes.len() == size
            )));
        } else {
            let Failure::Diagnostics(errors) = result.expect_err("first excess literal") else {
                panic!("budget diagnostic")
            };
            assert!(errors[0].message.contains("owned String literal ceiling"));
        }
    }
    let literal = "x".repeat(65_537);
    let source = format!(
        "function unused<T extends ZrynaValue>(input:T):T {{ const owner:String=\"{literal}\"; return input; }} export function root(input:i32):i32 {{ return input; }}"
    );
    let owned = claim(&[("main.zry", &source)]).expect("symbolic template has no executable bytes");
    assert!(owned.extensions.iter().flatten().all(|e| !matches!(
        e.operation,
        zryna_ir::generic_v1::owned_v2::raw::Operation::StringLiteral(_)
    )));
}

#[test]
fn small_source_cannot_replicate_literals_past_aggregate_wire_credit() {
    let literal = "x".repeat(65_536);
    let mut source = "function specialized<T extends ZrynaValue>(input:i32):i32 {".to_owned();
    for index in 0..8 {
        source.push_str(&format!("const owner{index}:String=\"{literal}\";"));
    }
    source.push_str("return input; } export function root(input:i32):i32 {");
    // 64 distinct instances retain exactly 32MiB; instance 65 must stop before its first copy.
    for index in 0..65 {
        let lhs = index / 9;
        let rhs = index % 9;
        let ty = format!(
            "{}Result<i32,{}bool{}>{}",
            "Option<".repeat(lhs),
            "Option<".repeat(rhs),
            ">".repeat(rhs),
            ">".repeat(lhs),
        );
        source.push_str(&format!("const v{index}:i32=specialized<{ty}>(input);"));
    }
    source.push_str("return input; }");
    assert!(source.len() < 800_000);
    let Failure::Diagnostics(errors) =
        claim(&[("main.zry", &source)]).expect_err("replication stops before literal 513")
    else {
        panic!("bounded specialization diagnostic")
    };
    assert!(errors[0].message.contains("owned source specialization aggregate ceiling"));
}
