//! Source-backed credit boundaries independently calculated from complete state inventories.
use super::tests::claim;
use std::fmt::Write;
use zryna_ir::generic_v1::Failure;

#[test]
fn nested_clone_exact_snapshot_credit_first_extra_and_recovery() {
    let mut body = String::new();
    for i in 0..400 {
        write!(body, "const p{i}:i32=0;").expect("fixture");
    }
    // 401 bindings; 419/433 key bytes. Eight/twelve snapshots and fourteen/twenty-two
    // definitions per discarded clone. Nested payload keys charge before either copy.
    // Option:10109+112*j, n73=1032293/n74=1050578.
    // Result:15467+264*j, n48=1040208/n49=1068347. Existing1048576 ceiling.
    for (ty, exact) in
        [("Option<Option<String>>", 73), ("Result<Option<String>,Option<String>>", 48)]
    {
        for count in [exact, exact + 1, 1] {
            let source = format!(
                "function churn(value:{ty}):i32 {{ {body} {} return 0; }} export function root(input:i32):i32 {{return input;}}",
                "clone(value);".repeat(count)
            );
            let result = claim(&[("main.zry", &source)]);
            if count == exact + 1 {
                assert!(
                    matches!(result,Err(Failure::Diagnostics(v)) if v[0].code=="ZRYNA-M7201"),
                    "{ty} first extra"
                );
            } else {
                result.expect("exact and pristine recovery");
            }
        }
    }
}
